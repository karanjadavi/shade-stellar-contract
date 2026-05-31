use super::*;
use crate::types::ChargeOutcome;
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::token::{StellarAssetClient, TokenClient};
use soroban_sdk::{Address, Env, String, Vec};

const MONTHLY: u64 = 2_592_000; // 30 days
const PLAN_AMOUNT: i128 = 1_000;

struct Fixture<'a> {
    env: Env,
    contract: Address,
    client: SubscriptionContractClient<'a>,
    merchant: Address,
    customer: Address,
    token: Address,
    plan_id: u64,
    sub_id: u64,
}

fn fund(env: &Env, token: &Address, to: &Address, amount: i128) {
    StellarAssetClient::new(env, token).mint(to, &amount);
}

fn approve(env: &Env, token: &Address, owner: &Address, spender: &Address, amount: i128) {
    let expiry = env.ledger().sequence() + 1_000_000;
    TokenClient::new(env, token).approve(owner, spender, &amount, &expiry);
}

fn balance(env: &Env, token: &Address, who: &Address) -> i128 {
    TokenClient::new(env, token).balance(who)
}

fn setup_with_grace(grace_period: u64) -> Fixture<'static> {
    let env = Env::default();
    env.mock_all_auths();

    // The "never charged" sentinel is `last_charged == 0`, so all tests must
    // run from a non-zero timestamp to avoid collisions when a charge lands.
    env.ledger().with_mut(|l| l.timestamp = 1_000_000);

    let contract = env.register(SubscriptionContract, ());
    let client = SubscriptionContractClient::new(&env, &contract);

    let admin = Address::generate(&env);
    client.initialize(&admin);

    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    client.add_accepted_token(&token);

    let merchant = Address::generate(&env);
    let customer = Address::generate(&env);

    let plan_id = client.create_plan(
        &merchant,
        &String::from_str(&env, "Pro Plan"),
        &token,
        &PLAN_AMOUNT,
        &MONTHLY,
        &None,
        &0,
    );
    if grace_period > 0 {
        client.set_plan_grace_period(&merchant, &plan_id, &grace_period);
    }

    let sub_id = client.subscribe(&customer, &plan_id);

    Fixture {
        env,
        contract,
        client,
        merchant,
        customer,
        token,
        plan_id,
        sub_id,
    }
}

fn advance_time(env: &Env, seconds: u64) {
    env.ledger().with_mut(|l| {
        l.timestamp += seconds;
    });
}

// ── Grace-period configuration ─────────────────────────────────────────────────

#[test]
fn test_default_plan_grace_period_is_zero() {
    let f = setup_with_grace(0);
    let plan = f.client.get_plan(&f.plan_id);
    assert_eq!(plan.grace_period, 0);
}

#[test]
fn test_set_plan_grace_period_updates_value() {
    let f = setup_with_grace(0);
    f.client
        .set_plan_grace_period(&f.merchant, &f.plan_id, &86_400);
    assert_eq!(f.client.get_plan(&f.plan_id).grace_period, 86_400);
}

#[test]
#[should_panic]
fn test_non_merchant_cannot_set_grace_period() {
    let f = setup_with_grace(0);
    let imposter = Address::generate(&f.env);
    f.client
        .set_plan_grace_period(&imposter, &f.plan_id, &86_400);
}

// ── Process charge outcomes ────────────────────────────────────────────────────

#[test]
fn test_first_charge_with_sufficient_allowance_returns_charged() {
    let f = setup_with_grace(86_400);
    fund(&f.env, &f.token, &f.customer, 5_000);
    approve(&f.env, &f.token, &f.customer, &f.contract, 5_000);

    let outcome = f.client.process_charge(&f.sub_id);
    assert_eq!(outcome, ChargeOutcome::Charged);
    assert_eq!(balance(&f.env, &f.token, &f.merchant), PLAN_AMOUNT);
}

#[test]
fn test_charge_before_interval_returns_not_due_yet() {
    let f = setup_with_grace(86_400);
    fund(&f.env, &f.token, &f.customer, 5_000);
    approve(&f.env, &f.token, &f.customer, &f.contract, 5_000);

    f.client.process_charge(&f.sub_id);
    // Don't advance — the next call is before the interval has elapsed.
    let outcome = f.client.process_charge(&f.sub_id);
    assert_eq!(outcome, ChargeOutcome::NotDueYet);
}

#[test]
fn test_failed_charge_with_zero_grace_terminates_immediately() {
    let f = setup_with_grace(0); // No grace.
                                 // No allowance → charge cannot succeed.

    let outcome = f.client.process_charge(&f.sub_id);
    assert_eq!(outcome, ChargeOutcome::Terminated);
    assert_eq!(
        f.client.get_subscription(&f.sub_id).status,
        SubscriptionStatus::Terminated
    );
}

#[test]
fn test_failed_charge_with_grace_enters_past_due() {
    let f = setup_with_grace(86_400);
    let outcome = f.client.process_charge(&f.sub_id);
    assert_eq!(outcome, ChargeOutcome::EnteredGrace);

    let sub = f.client.get_subscription(&f.sub_id);
    assert_eq!(sub.status, SubscriptionStatus::PastDue);
    assert_eq!(sub.past_due_since, f.env.ledger().timestamp());
}

#[test]
fn test_past_due_recovery_when_allowance_restored() {
    let f = setup_with_grace(86_400);
    // Step 1: charge fails → PastDue.
    f.client.process_charge(&f.sub_id);

    // Step 2: customer tops up & re-approves within the grace window.
    advance_time(&f.env, 3_600); // 1 hour later, still inside 24h grace.
    fund(&f.env, &f.token, &f.customer, 5_000);
    approve(&f.env, &f.token, &f.customer, &f.contract, 5_000);

    let outcome = f.client.process_charge(&f.sub_id);
    assert_eq!(outcome, ChargeOutcome::Recovered);

    let sub = f.client.get_subscription(&f.sub_id);
    assert_eq!(sub.status, SubscriptionStatus::Active);
    assert_eq!(sub.past_due_since, 0);
    assert_eq!(balance(&f.env, &f.token, &f.merchant), PLAN_AMOUNT);
}

#[test]
fn test_past_due_terminates_after_grace_expires() {
    let f = setup_with_grace(86_400);
    f.client.process_charge(&f.sub_id); // → PastDue

    // Advance past the grace window without recovery.
    advance_time(&f.env, 86_401);

    let outcome = f.client.process_charge(&f.sub_id);
    assert_eq!(outcome, ChargeOutcome::Terminated);
    assert_eq!(
        f.client.get_subscription(&f.sub_id).status,
        SubscriptionStatus::Terminated
    );
}

#[test]
fn test_past_due_within_grace_keeps_state_unchanged_when_still_short() {
    let f = setup_with_grace(86_400);
    f.client.process_charge(&f.sub_id); // → PastDue
    let entered_at = f.client.get_subscription(&f.sub_id).past_due_since;

    advance_time(&f.env, 3_600);
    let outcome = f.client.process_charge(&f.sub_id);
    assert_eq!(outcome, ChargeOutcome::EnteredGrace);

    // past_due_since is preserved across re-checks within the window.
    assert_eq!(
        f.client.get_subscription(&f.sub_id).past_due_since,
        entered_at
    );
}

#[test]
#[should_panic]
fn test_process_charge_panics_on_terminated() {
    let f = setup_with_grace(0);
    f.client.process_charge(&f.sub_id); // → Terminated
                                        // Calling again must panic — terminated is final.
    f.client.process_charge(&f.sub_id);
}

#[test]
#[should_panic]
fn test_process_charge_panics_on_cancelled() {
    let f = setup_with_grace(86_400);
    f.client.cancel_subscription(&f.customer, &f.sub_id);
    f.client.process_charge(&f.sub_id);
}

// ── enforce_grace ──────────────────────────────────────────────────────────────

#[test]
fn test_enforce_grace_terminates_after_window() {
    let f = setup_with_grace(86_400);
    f.client.process_charge(&f.sub_id); // → PastDue
    advance_time(&f.env, 86_401);

    f.client.enforce_grace(&f.sub_id);
    assert_eq!(
        f.client.get_subscription(&f.sub_id).status,
        SubscriptionStatus::Terminated
    );
}

#[test]
fn test_enforce_grace_is_idempotent_on_already_terminated() {
    let f = setup_with_grace(86_400);
    f.client.process_charge(&f.sub_id);
    advance_time(&f.env, 86_401);
    f.client.enforce_grace(&f.sub_id);
    // Second call is a no-op (no panic).
    f.client.enforce_grace(&f.sub_id);
}

#[test]
#[should_panic]
fn test_enforce_grace_panics_during_grace_window() {
    let f = setup_with_grace(86_400);
    f.client.process_charge(&f.sub_id);
    advance_time(&f.env, 1_000); // still inside grace window
    f.client.enforce_grace(&f.sub_id);
}

#[test]
#[should_panic]
fn test_enforce_grace_panics_when_active() {
    let f = setup_with_grace(86_400);
    fund(&f.env, &f.token, &f.customer, 5_000);
    approve(&f.env, &f.token, &f.customer, &f.contract, 5_000);
    f.client.process_charge(&f.sub_id);
    // Subscription is Active, not PastDue → cannot enforce grace.
    f.client.enforce_grace(&f.sub_id);
}

// ── Strict charge() respects state ─────────────────────────────────────────────

#[test]
#[should_panic]
fn test_strict_charge_panics_on_past_due() {
    let f = setup_with_grace(86_400);
    f.client.process_charge(&f.sub_id); // → PastDue
    f.client.charge(&f.sub_id);
}

// ── Multi-cycle billing: time-driven scheduler simulation ──────────────────────

#[test]
fn test_charge_executes_three_cycles_in_sequence() {
    // Simulate an off-chain scheduler waking up once per interval and
    // pulling funds. The merchant should accumulate three cycles' worth
    // and the customer's balance should drop the same amount.
    let f = setup_with_grace(0);
    fund(&f.env, &f.token, &f.customer, PLAN_AMOUNT * 3);
    approve(&f.env, &f.token, &f.customer, &f.contract, PLAN_AMOUNT * 3);

    // Cycle 1: due immediately because last_charged == 0.
    f.client.charge(&f.sub_id);
    advance_time(&f.env, MONTHLY);

    // Cycle 2.
    f.client.charge(&f.sub_id);
    advance_time(&f.env, MONTHLY);

    // Cycle 3.
    f.client.charge(&f.sub_id);

    assert_eq!(balance(&f.env, &f.token, &f.merchant), PLAN_AMOUNT * 3);
    assert_eq!(balance(&f.env, &f.token, &f.customer), 0);
    assert_eq!(
        f.client.get_subscription(&f.sub_id).status,
        SubscriptionStatus::Active
    );
}

#[test]
#[should_panic]
fn test_charge_panics_when_interval_not_yet_elapsed() {
    // Charging twice in immediate succession (no time advance) must
    // panic on the second call — the cycle hasn't elapsed.
    let f = setup_with_grace(0);
    fund(&f.env, &f.token, &f.customer, PLAN_AMOUNT * 2);
    approve(&f.env, &f.token, &f.customer, &f.contract, PLAN_AMOUNT * 2);

    f.client.charge(&f.sub_id);
    f.client.charge(&f.sub_id);
}

#[test]
fn test_charge_at_exact_interval_boundary_succeeds() {
    // The boundary check is `now < last_charged + interval` — i.e.
    // exactly at the interval is due, one second before is too early.
    let f = setup_with_grace(0);
    fund(&f.env, &f.token, &f.customer, PLAN_AMOUNT * 2);
    approve(&f.env, &f.token, &f.customer, &f.contract, PLAN_AMOUNT * 2);

    f.client.charge(&f.sub_id);
    advance_time(&f.env, MONTHLY); // exactly at the boundary
    f.client.charge(&f.sub_id);

    assert_eq!(balance(&f.env, &f.token, &f.merchant), PLAN_AMOUNT * 2);
}

// ── process_billing_cycle: batch scheduler entry point ─────────────────────────

#[test]
fn test_process_billing_cycle_charges_active_batch() {
    // A scheduler hands in three active subs all due for their first
    // charge — every one should be Charged and the merchant should hold
    // 3× the plan amount when the batch returns.
    let f = setup_with_grace(0);
    let customer_b = Address::generate(&f.env);
    let customer_c = Address::generate(&f.env);
    let sub_b = f.client.subscribe(&customer_b, &f.plan_id);
    let sub_c = f.client.subscribe(&customer_c, &f.plan_id);

    for c in [&f.customer, &customer_b, &customer_c] {
        fund(&f.env, &f.token, c, PLAN_AMOUNT);
        approve(&f.env, &f.token, c, &f.contract, PLAN_AMOUNT);
    }

    let mut ids: Vec<u64> = Vec::new(&f.env);
    ids.push_back(f.sub_id);
    ids.push_back(sub_b);
    ids.push_back(sub_c);

    let outcomes = f.client.process_billing_cycle(&ids);
    assert_eq!(outcomes.len(), 3);
    for outcome in outcomes.iter() {
        assert_eq!(outcome, ChargeOutcome::Charged);
    }
    assert_eq!(balance(&f.env, &f.token, &f.merchant), PLAN_AMOUNT * 3);
}

#[test]
fn test_process_billing_cycle_returns_skipped_for_cancelled() {
    let f = setup_with_grace(0);
    f.client.cancel_subscription(&f.customer, &f.sub_id);

    let mut ids: Vec<u64> = Vec::new(&f.env);
    ids.push_back(f.sub_id);
    let outcomes = f.client.process_billing_cycle(&ids);
    assert_eq!(outcomes.get(0).unwrap(), ChargeOutcome::Skipped);
}

#[test]
fn test_process_billing_cycle_returns_skipped_for_terminated() {
    let f = setup_with_grace(0);
    // No allowance + grace_period 0 → first process_charge terminates.
    f.client.process_charge(&f.sub_id);
    assert_eq!(
        f.client.get_subscription(&f.sub_id).status,
        SubscriptionStatus::Terminated
    );

    let mut ids: Vec<u64> = Vec::new(&f.env);
    ids.push_back(f.sub_id);
    let outcomes = f.client.process_billing_cycle(&ids);
    assert_eq!(outcomes.get(0).unwrap(), ChargeOutcome::Skipped);
}

#[test]
fn test_process_billing_cycle_handles_mixed_outcomes() {
    // One charged, one not-due-yet, one entered-grace, one skipped —
    // covers the full spread an off-chain scheduler can encounter in a
    // single sweep.
    let f = setup_with_grace(86_400);

    let customer_b = Address::generate(&f.env); // will charge
    let customer_c = Address::generate(&f.env); // not due yet
    let customer_d = Address::generate(&f.env); // enters grace
    let sub_b = f.client.subscribe(&customer_b, &f.plan_id);
    let sub_c = f.client.subscribe(&customer_c, &f.plan_id);
    let sub_d = f.client.subscribe(&customer_d, &f.plan_id);

    // sub_b will charge cleanly.
    fund(&f.env, &f.token, &customer_b, PLAN_AMOUNT);
    approve(&f.env, &f.token, &customer_b, &f.contract, PLAN_AMOUNT);

    // sub_c is already paid up; advancing only a fraction of MONTHLY
    // keeps it not-due.
    fund(&f.env, &f.token, &customer_c, PLAN_AMOUNT);
    approve(&f.env, &f.token, &customer_c, &f.contract, PLAN_AMOUNT);
    f.client.charge(&sub_c);

    // f.sub_id will be cancelled → Skipped.
    f.client.cancel_subscription(&f.customer, &f.sub_id);

    // sub_d has no allowance → EnteredGrace under the 86_400 grace plan.

    let mut ids: Vec<u64> = Vec::new(&f.env);
    ids.push_back(sub_b);
    ids.push_back(sub_c);
    ids.push_back(f.sub_id);
    ids.push_back(sub_d);

    let outcomes = f.client.process_billing_cycle(&ids);
    assert_eq!(outcomes.get(0).unwrap(), ChargeOutcome::Charged);
    assert_eq!(outcomes.get(1).unwrap(), ChargeOutcome::NotDueYet);
    assert_eq!(outcomes.get(2).unwrap(), ChargeOutcome::Skipped);
    assert_eq!(outcomes.get(3).unwrap(), ChargeOutcome::EnteredGrace);
}

#[test]
fn test_process_billing_cycle_does_not_panic_on_terminal_entries() {
    // Regression guard: a single dead entry must not abort the sweep —
    // the active entries after it must still be processed.
    let f = setup_with_grace(0);
    let customer_b = Address::generate(&f.env);
    let sub_b = f.client.subscribe(&customer_b, &f.plan_id);
    fund(&f.env, &f.token, &customer_b, PLAN_AMOUNT);
    approve(&f.env, &f.token, &customer_b, &f.contract, PLAN_AMOUNT);

    f.client.cancel_subscription(&f.customer, &f.sub_id);

    let mut ids: Vec<u64> = Vec::new(&f.env);
    ids.push_back(f.sub_id);
    ids.push_back(sub_b);
    let outcomes = f.client.process_billing_cycle(&ids);

    assert_eq!(outcomes.get(0).unwrap(), ChargeOutcome::Skipped);
    assert_eq!(outcomes.get(1).unwrap(), ChargeOutcome::Charged);
    assert_eq!(balance(&f.env, &f.token, &f.merchant), PLAN_AMOUNT);
}

#[test]
fn test_process_billing_cycle_empty_batch_returns_empty_vec() {
    let f = setup_with_grace(0);
    let outcomes = f.client.process_billing_cycle(&Vec::new(&f.env));
    assert_eq!(outcomes.len(), 0);
}

#[test]
fn test_process_billing_cycle_drives_grace_to_termination() {
    // Same subscription, two scheduler passes: first enters grace,
    // second (after the grace window expires) terminates.
    let f = setup_with_grace(86_400);

    let mut ids: Vec<u64> = Vec::new(&f.env);
    ids.push_back(f.sub_id);

    let first = f.client.process_billing_cycle(&ids);
    assert_eq!(first.get(0).unwrap(), ChargeOutcome::EnteredGrace);

    advance_time(&f.env, 86_401); // past the grace window

    let second = f.client.process_billing_cycle(&ids);
    assert_eq!(second.get(0).unwrap(), ChargeOutcome::Terminated);

    let third = f.client.process_billing_cycle(&ids);
    assert_eq!(third.get(0).unwrap(), ChargeOutcome::Skipped);
}

#[test]
fn test_process_billing_cycle_recovers_past_due_when_allowance_restored() {
    let f = setup_with_grace(86_400);

    let mut ids: Vec<u64> = Vec::new(&f.env);
    ids.push_back(f.sub_id);

    // Pass 1: no allowance → PastDue.
    f.client.process_billing_cycle(&ids);
    assert_eq!(
        f.client.get_subscription(&f.sub_id).status,
        SubscriptionStatus::PastDue
    );

    // Customer tops up within the grace window.
    advance_time(&f.env, 1_000);
    fund(&f.env, &f.token, &f.customer, PLAN_AMOUNT);
    approve(&f.env, &f.token, &f.customer, &f.contract, PLAN_AMOUNT);

    // Pass 2: pulls successfully → Recovered → Active.
    let outcomes = f.client.process_billing_cycle(&ids);
    assert_eq!(outcomes.get(0).unwrap(), ChargeOutcome::Recovered);
    assert_eq!(
        f.client.get_subscription(&f.sub_id).status,
        SubscriptionStatus::Active
    );
    assert_eq!(balance(&f.env, &f.token, &f.merchant), PLAN_AMOUNT);
}
