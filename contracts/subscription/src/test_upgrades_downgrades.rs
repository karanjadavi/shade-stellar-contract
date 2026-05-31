use super::*;
use crate::types::ChargeOutcome;
use soroban_sdk::testutils::{Address as _, Ledger as _, Events as _};
use soroban_sdk::token::{StellarAssetClient, TokenClient};
use soroban_sdk::{Address, Env, String, Symbol, TryIntoVal};

const MONTHLY: u64 = 2_592_000; // 30 days
const PLAN_AMOUNT: i128 = 1_000;

struct Fixture<'a> {
    env: Env,
    contract: Address,
    client: SubscriptionContractClient<'a>,
    merchant: Address,
    customer: Address,
    creator: Address,
    token: Address,
    plan_standard_id: u64,
    plan_premium_id: u64,
    plan_creator_id: u64,
    plan_trial_id: u64,
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

fn setup_all() -> Fixture<'static> {
    let env = Env::default();
    env.mock_all_auths();
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
    let creator = Address::generate(&env);

    // Plan Standard (no creator, no trial)
    let plan_standard_id = client.create_plan(
        &merchant,
        &String::from_str(&env, "Standard"),
        &token,
        &PLAN_AMOUNT,
        &MONTHLY,
        &None,
        &0,
    );

    // Plan Premium (no creator, no trial)
    let plan_premium_id = client.create_plan(
        &merchant,
        &String::from_str(&env, "Premium"),
        &token,
        &(PLAN_AMOUNT * 3),
        &MONTHLY,
        &None,
        &0,
    );

    // Plan Creator (with creator, no trial)
    let plan_creator_id = client.create_plan(
        &merchant,
        &String::from_str(&env, "Creator Plan"),
        &token,
        &PLAN_AMOUNT,
        &MONTHLY,
        &Some(creator.clone()),
        &0,
    );

    // Plan Trial (no creator, with 7-day trial)
    let plan_trial_id = client.create_plan(
        &merchant,
        &String::from_str(&env, "Trial Plan"),
        &token,
        &PLAN_AMOUNT,
        &MONTHLY,
        &None,
        &604_800, // 7 days in seconds
    );

    Fixture {
        env,
        contract,
        client,
        merchant,
        customer,
        creator,
        token,
        plan_standard_id,
        plan_premium_id,
        plan_creator_id,
        plan_trial_id,
    }
}

fn advance_time(env: &Env, seconds: u64) {
    env.ledger().with_mut(|l| {
        l.timestamp += seconds;
    });
}

// ── Creator Fee Distribution Tests ─────────────────────────────────────────────

#[test]
fn test_creator_receives_direct_funds() {
    let f = setup_all();
    fund(&f.env, &f.token, &f.customer, 5_000);
    approve(&f.env, &f.token, &f.customer, &f.contract, 5_000);

    let sub_id = f.client.subscribe(&f.customer, &f.plan_creator_id);
    let outcome = f.client.process_charge(&sub_id);

    assert_eq!(outcome, ChargeOutcome::Charged);
    assert_eq!(balance(&f.env, &f.token, &f.creator), PLAN_AMOUNT);
    assert_eq!(balance(&f.env, &f.token, &f.merchant), 0);
}

#[test]
fn test_refund_pulled_from_creator() {
    let f = setup_all();
    fund(&f.env, &f.token, &f.customer, 5_000);
    approve(&f.env, &f.token, &f.customer, &f.contract, 5_000);

    let sub_id = f.client.subscribe(&f.customer, &f.plan_creator_id);
    f.client.charge(&sub_id);

    // Creator funds/approves refund
    approve(&f.env, &f.token, &f.creator, &f.contract, PLAN_AMOUNT);

    let customer_before = balance(&f.env, &f.token, &f.customer);
    let creator_before = balance(&f.env, &f.token, &f.creator);

    // Cancel with refund
    f.client.cancel_with_prorated_refund(&f.customer, &sub_id);

    assert_eq!(balance(&f.env, &f.token, &f.customer), customer_before + PLAN_AMOUNT);
    assert_eq!(balance(&f.env, &f.token, &f.creator), creator_before - PLAN_AMOUNT);
}

// ── Webhook / Event Triggers Tests ─────────────────────────────────────────────

#[test]
fn test_sub_renewed_event_emitted() {
    let f = setup_all();
    fund(&f.env, &f.token, &f.customer, 5_000);
    approve(&f.env, &f.token, &f.customer, &f.contract, 5_000);

    let sub_id = f.client.subscribe(&f.customer, &f.plan_standard_id);
    f.client.charge(&sub_id);

    let events = f.env.events().all();
    assert!(!events.is_empty());

    let mut found = false;
    for event in events.iter() {
        let (_, topics, _) = event;
        if topics.len() > 0 {
            let event_name: Symbol = topics.get(0).unwrap().try_into_val(&f.env).unwrap();
            if event_name == Symbol::new(&f.env, "sub_renewed") {
                found = true;
                break;
            }
        }
    }
    assert!(found, "SubRenewed event was not found");
}

#[test]
fn test_sub_expired_event_emitted_on_cancel() {
    let f = setup_all();
    let sub_id = f.client.subscribe(&f.customer, &f.plan_standard_id);
    f.client.cancel_subscription(&f.customer, &sub_id);

    let events = f.env.events().all();
    assert!(!events.is_empty());

    let mut found = false;
    for event in events.iter() {
        let (_, topics, _) = event;
        if topics.len() > 0 {
            let event_name: Symbol = topics.get(0).unwrap().try_into_val(&f.env).unwrap();
            if event_name == Symbol::new(&f.env, "sub_expired") {
                found = true;
                break;
            }
        }
    }
    assert!(found, "SubExpired event was not found");
}

// ── Plan Upgrades & Downgrades Tests ───────────────────────────────────────────

#[test]
fn test_instant_upgrade_cost_calculation_and_execution() {
    let f = setup_all();
    fund(&f.env, &f.token, &f.customer, 10_000);
    approve(&f.env, &f.token, &f.customer, &f.contract, 10_000);

    let sub_id = f.client.subscribe(&f.customer, &f.plan_standard_id);
    f.client.charge(&sub_id); // Charged standard (1_000)

    // Advance 10 days out of 30. Remaining: 20 days.
    // Prorated refund for remaining 20 days: 1_000 * 20 / 30 = 666.
    // Premium plan costs 3_000.
    // Net upgrade cost: 3_000 - 666 = 2_334.
    advance_time(&f.env, 864_000); // 10 days in seconds

    let customer_before = balance(&f.env, &f.token, &f.customer); // 9_000
    let merchant_before = balance(&f.env, &f.token, &f.merchant); // 1_000

    f.client.upgrade_subscription(&f.customer, &sub_id, &f.plan_premium_id);

    // Premium plan is now active
    let sub = f.client.get_subscription(&sub_id);
    assert_eq!(sub.plan_id, f.plan_premium_id);
    assert_eq!(sub.last_charged, f.env.ledger().timestamp());

    // Balance changes
    let customer_after = balance(&f.env, &f.token, &f.customer);
    let merchant_after = balance(&f.env, &f.token, &f.merchant);

    // Upgrade cost should be 2_334
    assert_eq!(customer_before - customer_after, 2_334);
    assert_eq!(merchant_after - merchant_before, 2_334);
}

#[test]
fn test_deferred_downgrade_execution() {
    let f = setup_all();
    fund(&f.env, &f.token, &f.customer, 10_000);
    approve(&f.env, &f.token, &f.customer, &f.contract, 10_000);

    let sub_id = f.client.subscribe(&f.customer, &f.plan_premium_id);
    f.client.charge(&sub_id); // Charged premium (3_000)

    // Schedule downgrade to standard (1_000)
    f.client.downgrade_subscription(&f.customer, &sub_id, &f.plan_standard_id);

    // Plan should still be premium before cycle ends
    let sub = f.client.get_subscription(&sub_id);
    assert_eq!(sub.plan_id, f.plan_premium_id);

    // Advance 30 days (end of cycle)
    advance_time(&f.env, MONTHLY);

    let merchant_before = balance(&f.env, &f.token, &f.merchant);
    // Charge next cycle
    f.client.charge(&sub_id);

    // Verify downgraded and standard amount charged
    let sub_after = f.client.get_subscription(&sub_id);
    assert_eq!(sub_after.plan_id, f.plan_standard_id);
    assert_eq!(balance(&f.env, &f.token, &f.merchant), merchant_before + PLAN_AMOUNT);
}

// ── Trial Period Tests ─────────────────────────────────────────────────────────

#[test]
fn test_trial_period_defers_pull() {
    let f = setup_all();
    fund(&f.env, &f.token, &f.customer, 5_000);
    approve(&f.env, &f.token, &f.customer, &f.contract, 5_000);

    let sub_id = f.client.subscribe(&f.customer, &f.plan_trial_id);

    // Try to charge immediately -> fails
    let res = f.client.try_charge(&sub_id);
    assert!(res.is_err());

    // Advance 6 days -> fails
    advance_time(&f.env, 518_400); // 6 days
    let res = f.client.try_charge(&sub_id);
    assert!(res.is_err());

    // Advance to 7.1 days -> succeeds
    advance_time(&f.env, 100_000); // 7.1 days total
    f.client.charge(&sub_id);
    assert_eq!(balance(&f.env, &f.token, &f.merchant), PLAN_AMOUNT);
}
