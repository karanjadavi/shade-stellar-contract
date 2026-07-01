//! Comprehensive tests for automated refund flows (#191 / #304 / #307):
//! `claim_refund` (per-contributor) and `batch_refund` (campaign-wide).

use crate::*;
use soroban_sdk::testutils::{Address as _, Events as _, Ledger as _};
use soroban_sdk::token::StellarAssetClient;
use soroban_sdk::{Address, Env, Error, Map, Symbol, TryIntoVal, Val};

const CAMPAIGN_NOT_ENDED: u32 = 7;
const GOAL_REACHED: u32 = 9;
const NO_PLEDGE: u32 = 10;
const NOT_INITIALIZED: u32 = 2;
const REFUND_ALREADY_PROCESSED: u32 = 25;

fn contract_error(code: u32) -> Error {
    Error::from_contract_error(code)
}

struct RefundFixture<'a> {
    env: Env,
    contract: Address,
    client: CrowdfundContractClient<'a>,
    token: Address,
}

fn setup_failed_campaign(
    goal: i128,
    deadline_offset: u64,
) -> (RefundFixture<'static>, Address, u64) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000_000);

    let contract = env.register(CrowdfundContract, ());
    let client = CrowdfundContractClient::new(&env, &contract);
    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    let organizer = Address::generate(&env);
    let contributor = Address::generate(&env);
    let deadline = env.ledger().timestamp() + deadline_offset;

    client.init_campaign(&organizer, &token, &goal, &deadline);

    let fixture = RefundFixture {
        env,
        contract,
        client,
        token,
    };
    (fixture, contributor, deadline)
}

fn advance_past_deadline(env: &Env, deadline: u64) {
    env.ledger().with_mut(|l| l.timestamp = deadline + 1);
}

fn mint_and_contribute(
    fixture: &RefundFixture,
    contributor: &Address,
    amount: i128,
) {
    StellarAssetClient::new(&fixture.env, &fixture.token).mint(contributor, &amount);
    fixture.client.contribute(contributor, &amount);
}

fn find_event_data(
    env: &Env,
    contract: &Address,
    event_name: &str,
) -> Map<Symbol, Val> {
    let events = env.events().all();
    for i in 0..events.len() {
        let (event_contract, topics, data) = events.get(i).unwrap();
        if event_contract != *contract || topics.len() == 0 {
            continue;
        }
        let name: Symbol = topics.get(0).unwrap().try_into_val(env).unwrap();
        if name == Symbol::new(env, event_name) {
            return data.try_into_val(env).unwrap();
        }
    }
    panic!("event {event_name} not found");
}

fn assert_refund_claimed_event(
    env: &Env,
    contract: &Address,
    expected_contributor: &Address,
    expected_amount: i128,
) {
    let data = find_event_data(env, contract, "refund_claimed_event");
    let contributor: Address = data
        .get(Symbol::new(env, "contributor"))
        .unwrap()
        .try_into_val(env)
        .unwrap();
    let amount: i128 = data
        .get(Symbol::new(env, "amount"))
        .unwrap()
        .try_into_val(env)
        .unwrap();
    assert_eq!(contributor, expected_contributor.clone());
    assert_eq!(amount, expected_amount);
}

fn assert_batch_refund_processed_event(
    env: &Env,
    contract: &Address,
    expected_total: i128,
    expected_count: u32,
) {
    let data = find_event_data(env, contract, "batch_refund_processed_event");
    let total_refunded: i128 = data
        .get(Symbol::new(env, "total_refunded"))
        .unwrap()
        .try_into_val(env)
        .unwrap();
    let contributor_count: u32 = data
        .get(Symbol::new(env, "contributor_count"))
        .unwrap()
        .try_into_val(env)
        .unwrap();
    assert_eq!(total_refunded, expected_total);
    assert_eq!(contributor_count, expected_count);
}

// ── Happy path: individual claim_refund ──────────────────────────────────────

#[test]
fn test_claim_refund_happy_path_transfers_and_zeros_pledge() {
    let (f, contributor, deadline) = setup_failed_campaign(5_000, 100);
    mint_and_contribute(&f, &contributor, 2_500);
    advance_past_deadline(&f.env, deadline);

    let token_client = StellarAssetClient::new(&f.env, &f.token);
    let before = token_client.balance(&contributor);
    f.client.claim_refund(&contributor);

    assert_eq!(token_client.balance(&contributor) - before, 2_500);
    assert_eq!(f.client.pledge_of(&contributor), 0);
    assert_refund_claimed_event(&f.env, &f.contract, &contributor, 2_500);
}

#[test]
fn test_batch_refund_happy_path_refunds_all_contributors() {
    let (f, contributor1, deadline) = setup_failed_campaign(10_000, 100);
    let contributor2 = Address::generate(&f.env);
    mint_and_contribute(&f, &contributor1, 3_000);
    mint_and_contribute(&f, &contributor2, 2_000);
    advance_past_deadline(&f.env, deadline);

    let token_client = StellarAssetClient::new(&f.env, &f.token);
    let before1 = token_client.balance(&contributor1);
    let before2 = token_client.balance(&contributor2);

    f.client.batch_refund();

    assert_eq!(token_client.balance(&contributor1) - before1, 3_000);
    assert_eq!(token_client.balance(&contributor2) - before2, 2_000);
    assert_eq!(f.client.pledge_of(&contributor1), 0);
    assert_eq!(f.client.pledge_of(&contributor2), 0);
    assert_batch_refund_processed_event(&f.env, &f.contract, 5_000, 2);
}

#[test]
fn test_batch_refund_requires_no_caller_auth() {
    let (f, contributor, deadline) = setup_failed_campaign(5_000, 100);
    mint_and_contribute(&f, &contributor, 1_000);
    advance_past_deadline(&f.env, deadline);

    // Permissionless: succeeds without organizer or contributor authorization.
    assert!(f.client.try_batch_refund().is_ok());
    assert_eq!(f.client.pledge_of(&contributor), 0);
}

// ── Unauthorized / malicious access ───────────────────────────────────────────

#[test]
#[should_panic]
fn test_claim_refund_requires_contributor_auth() {
    let env = Env::default();
    let contract = env.register(CrowdfundContract, ());
    let client = CrowdfundContractClient::new(&env, &contract);
    env.ledger().with_mut(|l| l.timestamp = 1_000_000);

    let organizer = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(organizer.clone())
        .address();
    let contributor = Address::generate(&env);
    let deadline = env.ledger().timestamp() + 100;

    client.init_campaign(&organizer, &token, &5_000, &deadline);
    client.claim_refund(&contributor);
}

#[test]
fn test_non_backer_cannot_claim_refund() {
    let (f, contributor, deadline) = setup_failed_campaign(5_000, 100);
    mint_and_contribute(&f, &contributor, 1_000);
    advance_past_deadline(&f.env, deadline);

    let stranger = Address::generate(&f.env);
    let result = f.client.try_claim_refund(&stranger);
    assert_eq!(result, Err(Ok(contract_error(NO_PLEDGE))));
}

// ── Boundary values ───────────────────────────────────────────────────────────

#[test]
fn test_refund_at_exact_deadline_still_blocked() {
    let (f, contributor, deadline) = setup_failed_campaign(5_000, 100);
    mint_and_contribute(&f, &contributor, 1_000);
    f.env.ledger().with_mut(|l| l.timestamp = deadline);

    let result = f.client.try_claim_refund(&contributor);
    assert_eq!(result, Err(Ok(contract_error(CAMPAIGN_NOT_ENDED))));
    assert_eq!(f.client.pledge_of(&contributor), 1_000);
}

#[test]
fn test_refund_one_second_after_deadline_succeeds() {
    let (f, contributor, deadline) = setup_failed_campaign(5_000, 100);
    mint_and_contribute(&f, &contributor, 1_000);
    advance_past_deadline(&f.env, deadline);

    f.client.claim_refund(&contributor);
    assert_eq!(f.client.pledge_of(&contributor), 0);
}

#[test]
fn test_refund_when_raised_one_below_goal() {
    let (f, contributor, deadline) = setup_failed_campaign(1_000, 100);
    mint_and_contribute(&f, &contributor, 999);
    advance_past_deadline(&f.env, deadline);

    f.client.claim_refund(&contributor);
    assert_eq!(f.client.pledge_of(&contributor), 0);
}

#[test]
fn test_refund_blocked_when_goal_exactly_reached() {
    let (f, contributor, deadline) = setup_failed_campaign(1_000, 100);
    mint_and_contribute(&f, &contributor, 1_000);
    advance_past_deadline(&f.env, deadline);

    let result = f.client.try_claim_refund(&contributor);
    assert_eq!(result, Err(Ok(contract_error(GOAL_REACHED))));
    assert_eq!(f.client.pledge_of(&contributor), 1_000);
}

#[test]
fn test_uninitialized_campaign_claim_refund_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let contract = env.register(CrowdfundContract, ());
    let client = CrowdfundContractClient::new(&env, &contract);
    let contributor = Address::generate(&env);

    let result = client.try_claim_refund(&contributor);
    assert_eq!(result, Err(Ok(contract_error(NOT_INITIALIZED))));
}

#[test]
fn test_uninitialized_campaign_batch_refund_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let contract = env.register(CrowdfundContract, ());
    let client = CrowdfundContractClient::new(&env, &contract);

    let result = client.try_batch_refund();
    assert_eq!(result, Err(Ok(contract_error(NOT_INITIALIZED))));
}

// ── Storage rollback on panic ─────────────────────────────────────────────────

#[test]
fn test_claim_refund_panic_preserves_pledge_storage() {
    let (f, contributor, deadline) = setup_failed_campaign(5_000, 100);
    mint_and_contribute(&f, &contributor, 1_500);

    let result = f.client.try_claim_refund(&contributor);
    assert_eq!(result, Err(Ok(contract_error(CAMPAIGN_NOT_ENDED))));
    assert_eq!(f.client.pledge_of(&contributor), 1_500);
    assert_eq!(f.client.raised(), 1_500);

    advance_past_deadline(&f.env, deadline);
    f.client.claim_refund(&contributor);
    assert_eq!(f.client.pledge_of(&contributor), 0);
}

#[test]
fn test_batch_refund_panic_preserves_pledges_and_allows_retry() {
    let (f, contributor, deadline) = setup_failed_campaign(5_000, 100);
    mint_and_contribute(&f, &contributor, 2_000);

    let result = f.client.try_batch_refund();
    assert_eq!(result, Err(Ok(contract_error(CAMPAIGN_NOT_ENDED))));
    assert_eq!(f.client.pledge_of(&contributor), 2_000);

    advance_past_deadline(&f.env, deadline);
    f.client.batch_refund();
    assert_eq!(f.client.pledge_of(&contributor), 0);
}

#[test]
fn test_batch_refund_goal_reached_panic_preserves_state() {
    let (f, contributor, deadline) = setup_failed_campaign(500, 100);
    mint_and_contribute(&f, &contributor, 500);
    advance_past_deadline(&f.env, deadline);

    let token_client = StellarAssetClient::new(&f.env, &f.token);
    let contract_balance_before = token_client.balance(&f.contract);

    let result = f.client.try_batch_refund();
    assert_eq!(result, Err(Ok(contract_error(GOAL_REACHED))));
    assert_eq!(f.client.pledge_of(&contributor), 500);
    assert_eq!(token_client.balance(&f.contract), contract_balance_before);
}

// ── State transitions & edge cases ────────────────────────────────────────────

#[test]
fn test_double_claim_refund_fails_after_first_success() {
    let (f, contributor, deadline) = setup_failed_campaign(5_000, 100);
    mint_and_contribute(&f, &contributor, 800);
    advance_past_deadline(&f.env, deadline);

    f.client.claim_refund(&contributor);
    let result = f.client.try_claim_refund(&contributor);
    assert_eq!(result, Err(Ok(contract_error(NO_PLEDGE))));
}

#[test]
fn test_double_batch_refund_fails() {
    let (f, contributor, deadline) = setup_failed_campaign(5_000, 100);
    mint_and_contribute(&f, &contributor, 600);
    advance_past_deadline(&f.env, deadline);

    f.client.batch_refund();
    let result = f.client.try_batch_refund();
    assert_eq!(result, Err(Ok(contract_error(REFUND_ALREADY_PROCESSED))));
}

#[test]
fn test_mixed_individual_then_batch_refunds_remaining_contributors() {
    let (f, contributor1, deadline) = setup_failed_campaign(10_000, 100);
    let contributor2 = Address::generate(&f.env);
    mint_and_contribute(&f, &contributor1, 4_000);
    mint_and_contribute(&f, &contributor2, 3_000);
    advance_past_deadline(&f.env, deadline);

    let token_client = StellarAssetClient::new(&f.env, &f.token);
    f.client.claim_refund(&contributor1);
    assert_eq!(f.client.pledge_of(&contributor1), 0);

    let before2 = token_client.balance(&contributor2);
    f.client.batch_refund();
    assert_eq!(token_client.balance(&contributor2) - before2, 3_000);
    assert_eq!(f.client.pledge_of(&contributor2), 0);
    assert_batch_refund_processed_event(&f.env, &f.contract, 3_000, 2);
}

#[test]
fn test_batch_refund_skips_zero_pledge_contributors_in_total() {
    let (f, contributor1, deadline) = setup_failed_campaign(10_000, 100);
    let contributor2 = Address::generate(&f.env);
    mint_and_contribute(&f, &contributor1, 1_000);
    mint_and_contribute(&f, &contributor2, 500);
    advance_past_deadline(&f.env, deadline);

    f.client.claim_refund(&contributor2);
    assert_eq!(f.client.pledge_of(&contributor2), 0);

    let token_client = StellarAssetClient::new(&f.env, &f.token);
    let before1 = token_client.balance(&contributor1);
    f.client.batch_refund();
    assert_eq!(token_client.balance(&contributor1) - before1, 1_000);
    assert_batch_refund_processed_event(&f.env, &f.contract, 1_000, 2);
}

#[test]
fn test_batch_refund_with_matching_pool_pledge_amounts() {
    let (f, contributor, deadline) = setup_failed_campaign(10_000, 100);
    let sponsor = Address::generate(&f.env);
    let token_client = StellarAssetClient::new(&f.env, &f.token);
    token_client.mint(&sponsor, 300);
    token_client.mint(&contributor, 500);
    f.client.fund_matching_pool(&sponsor, &300);
    f.client.contribute(&contributor, &500);
    // Pledge includes 300 matched: 800 total per contributor accounting.
    assert_eq!(f.client.pledge_of(&contributor), 800);
    advance_past_deadline(&f.env, deadline);

    let before = token_client.balance(&contributor);
    f.client.batch_refund();
    assert_eq!(token_client.balance(&contributor) - before, 800);
    assert_batch_refund_processed_event(&f.env, &f.contract, 800, 1);
}

#[test]
fn test_batch_refund_empty_contributor_list_succeeds() {
    let (f, _contributor, deadline) = setup_failed_campaign(5_000, 100);
    advance_past_deadline(&f.env, deadline);

    f.client.batch_refund();
    assert_batch_refund_processed_event(&f.env, &f.contract, 0, 0);

    let result = f.client.try_batch_refund();
    assert_eq!(result, Err(Ok(contract_error(REFUND_ALREADY_PROCESSED))));
}

#[test]
fn test_successful_campaign_blocks_both_refund_paths() {
    let (f, contributor, deadline) = setup_failed_campaign(1_000, 100);
    mint_and_contribute(&f, &contributor, 1_000);
    advance_past_deadline(&f.env, deadline);

    assert!(f.client.goal_reached());
    assert_eq!(f.client.try_claim_refund(&contributor), Err(Ok(contract_error(GOAL_REACHED))));
    assert_eq!(f.client.try_batch_refund(), Err(Ok(contract_error(GOAL_REACHED))));
}
