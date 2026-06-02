use super::*;
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::token::StellarAssetClient;
use soroban_sdk::{vec, Address, Env};

fn setup() -> (Env, Address, CrowdfundContractClient<'static>, Address, Address, Address) {
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

    (env, contract, client, token, organizer, contributor)
}

// ── Existing init / contribute tests ─────────────────────────────────────────

#[test]
fn test_init_campaign_stores_goal_and_deadline() {
    let (env, _contract, client, token, organizer, _) = setup();
    let goal = 10_000_i128;
    let deadline = env.ledger().timestamp() + 86_400;

    client.init_campaign(&organizer, &token, &goal, &deadline);

    assert_eq!(client.goal(), goal);
    assert_eq!(client.deadline(), deadline);
    assert_eq!(client.raised(), 0);
    assert_eq!(client.organizer(), organizer);
    assert!(!client.goal_reached());
}

#[test]
#[should_panic]
fn test_double_init_panics() {
    let (env, _contract, client, token, organizer, _) = setup();
    let deadline = env.ledger().timestamp() + 86_400;
    client.init_campaign(&organizer, &token, &10_000, &deadline);
    client.init_campaign(&organizer, &token, &10_000, &deadline);
}

#[test]
#[should_panic]
fn test_zero_goal_panics() {
    let (env, _contract, client, token, organizer, _) = setup();
    let deadline = env.ledger().timestamp() + 86_400;
    client.init_campaign(&organizer, &token, &0, &deadline);
}

#[test]
#[should_panic]
fn test_past_deadline_panics() {
    let (env, _contract, client, token, organizer, _) = setup();
    client.init_campaign(&organizer, &token, &1_000, &(env.ledger().timestamp() - 1));
}

#[test]
fn test_contribute_increases_raised() {
    let (env, _contract, client, token, organizer, contributor) = setup();
    let deadline = env.ledger().timestamp() + 86_400;
    client.init_campaign(&organizer, &token, &5_000, &deadline);

    StellarAssetClient::new(&env, &token).mint(&contributor, &3_000);
    client.contribute(&contributor, &3_000);

    assert_eq!(client.raised(), 3_000);
    assert!(!client.goal_reached());
}

#[test]
fn test_goal_reached_when_fully_funded() {
    let (env, _contract, client, token, organizer, contributor) = setup();
    let deadline = env.ledger().timestamp() + 86_400;
    client.init_campaign(&organizer, &token, &1_000, &deadline);

    StellarAssetClient::new(&env, &token).mint(&contributor, &1_000);
    client.contribute(&contributor, &1_000);

    assert!(client.goal_reached());
}

#[test]
#[should_panic]
fn test_contribute_after_deadline_panics() {
    let (env, _contract, client, token, organizer, contributor) = setup();
    let deadline = env.ledger().timestamp() + 100;
    client.init_campaign(&organizer, &token, &5_000, &deadline);

    env.ledger().with_mut(|l| l.timestamp += 200);
    StellarAssetClient::new(&env, &token).mint(&contributor, &1_000);
    client.contribute(&contributor, &1_000);
}

// ── #302 – Pledge tracking and accounting ────────────────────────────────────

#[test]
fn test_pledge_tracked_per_contributor() {
    let (env, _contract, client, token, organizer, contributor) = setup();
    let deadline = env.ledger().timestamp() + 86_400;
    client.init_campaign(&organizer, &token, &10_000, &deadline);

    StellarAssetClient::new(&env, &token).mint(&contributor, &4_000);
    client.contribute(&contributor, &1_500);
    client.contribute(&contributor, &2_500);

    assert_eq!(client.pledge_of(&contributor), 4_000);
    assert_eq!(client.raised(), 4_000);
}

#[test]
fn test_multiple_contributors_sum_correctly() {
    let (env, _contract, client, token, organizer, contributor) = setup();
    let contributor2 = Address::generate(&env);
    let deadline = env.ledger().timestamp() + 86_400;
    client.init_campaign(&organizer, &token, &10_000, &deadline);

    StellarAssetClient::new(&env, &token).mint(&contributor, &3_000);
    StellarAssetClient::new(&env, &token).mint(&contributor2, &7_000);
    client.contribute(&contributor, &3_000);
    client.contribute(&contributor2, &7_000);

    assert_eq!(client.raised(), 10_000);
    assert_eq!(client.pledge_of(&contributor), 3_000);
    assert_eq!(client.pledge_of(&contributor2), 7_000);
    assert!(client.goal_reached());
}

#[test]
fn test_pledge_of_returns_zero_for_non_contributor() {
    let (env, _contract, client, token, organizer, contributor) = setup();
    let non_contributor = Address::generate(&env);
    let deadline = env.ledger().timestamp() + 86_400;
    client.init_campaign(&organizer, &token, &1_000, &deadline);

    StellarAssetClient::new(&env, &token).mint(&contributor, &500);
    client.contribute(&contributor, &500);

    assert_eq!(client.pledge_of(&non_contributor), 0);
}

// ── #303 – Successful campaign execution ─────────────────────────────────────

#[test]
fn test_execute_campaign_transfers_to_organizer() {
    let (env, _contract, client, token, organizer, contributor) = setup();
    let deadline = env.ledger().timestamp() + 100;
    client.init_campaign(&organizer, &token, &1_000, &deadline);

    StellarAssetClient::new(&env, &token).mint(&contributor, &1_000);
    client.contribute(&contributor, &1_000);

    // Advance past deadline.
    env.ledger().with_mut(|l| l.timestamp += 200);
    let token_client = StellarAssetClient::new(&env, &token);
    let before = token_client.balance(&organizer);
    client.execute_campaign();
    let after = token_client.balance(&organizer);

    assert_eq!(after - before, 1_000);
    assert!(client.is_executed());
}

#[test]
#[should_panic]
fn test_execute_before_deadline_panics() {
    let (env, _contract, client, token, organizer, contributor) = setup();
    let deadline = env.ledger().timestamp() + 86_400;
    client.init_campaign(&organizer, &token, &500, &deadline);

    StellarAssetClient::new(&env, &token).mint(&contributor, &500);
    client.contribute(&contributor, &500);

    // Deadline not yet passed.
    client.execute_campaign();
}

#[test]
#[should_panic]
fn test_execute_when_goal_not_met_panics() {
    let (env, _contract, client, token, organizer, contributor) = setup();
    let deadline = env.ledger().timestamp() + 100;
    client.init_campaign(&organizer, &token, &5_000, &deadline);

    StellarAssetClient::new(&env, &token).mint(&contributor, &1_000);
    client.contribute(&contributor, &1_000);

    env.ledger().with_mut(|l| l.timestamp += 200);
    client.execute_campaign();
}

#[test]
#[should_panic]
fn test_execute_twice_panics() {
    let (env, _contract, client, token, organizer, contributor) = setup();
    let deadline = env.ledger().timestamp() + 100;
    client.init_campaign(&organizer, &token, &500, &deadline);

    StellarAssetClient::new(&env, &token).mint(&contributor, &500);
    client.contribute(&contributor, &500);
    env.ledger().with_mut(|l| l.timestamp += 200);

    client.execute_campaign();
    client.execute_campaign();
}

// ── #304 – Failed campaign refunds ───────────────────────────────────────────

#[test]
fn test_claim_refund_returns_pledge_on_failed_campaign() {
    let (env, _contract, client, token, organizer, contributor) = setup();
    let deadline = env.ledger().timestamp() + 100;
    client.init_campaign(&organizer, &token, &5_000, &deadline);

    StellarAssetClient::new(&env, &token).mint(&contributor, &1_000);
    client.contribute(&contributor, &1_000);

    env.ledger().with_mut(|l| l.timestamp += 200);

    let token_client = StellarAssetClient::new(&env, &token);
    let before = token_client.balance(&contributor);
    client.claim_refund(&contributor);
    let after = token_client.balance(&contributor);

    assert_eq!(after - before, 1_000);
    // Pledge zeroed after refund.
    assert_eq!(client.pledge_of(&contributor), 0);
}

#[test]
#[should_panic]
fn test_claim_refund_before_deadline_panics() {
    let (env, _contract, client, token, organizer, contributor) = setup();
    let deadline = env.ledger().timestamp() + 86_400;
    client.init_campaign(&organizer, &token, &5_000, &deadline);

    StellarAssetClient::new(&env, &token).mint(&contributor, &1_000);
    client.contribute(&contributor, &1_000);

    client.claim_refund(&contributor);
}

#[test]
#[should_panic]
fn test_claim_refund_on_successful_campaign_panics() {
    let (env, _contract, client, token, organizer, contributor) = setup();
    let deadline = env.ledger().timestamp() + 100;
    client.init_campaign(&organizer, &token, &1_000, &deadline);

    StellarAssetClient::new(&env, &token).mint(&contributor, &1_000);
    client.contribute(&contributor, &1_000);
    env.ledger().with_mut(|l| l.timestamp += 200);

    client.claim_refund(&contributor);
}

#[test]
#[should_panic]
fn test_claim_refund_with_no_pledge_panics() {
    let (env, _contract, client, token, organizer, _contributor) = setup();
    let non_backer = Address::generate(&env);
    let deadline = env.ledger().timestamp() + 100;
    client.init_campaign(&organizer, &token, &5_000, &deadline);

    env.ledger().with_mut(|l| l.timestamp += 200);
    client.claim_refund(&non_backer);
}

#[test]
#[should_panic]
fn test_double_refund_panics() {
    let (env, _contract, client, token, organizer, contributor) = setup();
    let deadline = env.ledger().timestamp() + 100;
    client.init_campaign(&organizer, &token, &5_000, &deadline);

    StellarAssetClient::new(&env, &token).mint(&contributor, &1_000);
    client.contribute(&contributor, &1_000);
    env.ledger().with_mut(|l| l.timestamp += 200);

    client.claim_refund(&contributor);
    client.claim_refund(&contributor);
}

// ── #306 – Stretch goals tracking ────────────────────────────────────────────

#[test]
fn test_stretch_goals_activate_when_threshold_crossed() {
    let (env, _contract, client, token, organizer, contributor) = setup();
    let deadline = env.ledger().timestamp() + 86_400;
    client.init_campaign(&organizer, &token, &1_000, &deadline);
    client.set_stretch_goals(&vec![&env, 2_000_i128, 5_000_i128]);

    StellarAssetClient::new(&env, &token).mint(&contributor, &5_000);
    client.contribute(&contributor, &2_000);
    // First stretch goal crossed at 2_000.

    client.contribute(&contributor, &3_000);
    // Second stretch goal crossed at 5_000.

    assert_eq!(client.raised(), 5_000);
}

#[test]
fn test_stretch_goal_not_triggered_before_threshold() {
    let (env, _contract, client, token, organizer, contributor) = setup();
    let deadline = env.ledger().timestamp() + 86_400;
    client.init_campaign(&organizer, &token, &1_000, &deadline);
    client.set_stretch_goals(&vec![&env, 3_000_i128]);

    StellarAssetClient::new(&env, &token).mint(&contributor, &1_000);
    client.contribute(&contributor, &1_000);

    // Only 1_000 raised — stretch goal at 3_000 not yet triggered.
    assert_eq!(client.raised(), 1_000);
}

#[test]
#[should_panic]
fn test_set_stretch_goals_non_ascending_panics() {
    let (env, _contract, client, token, organizer, _) = setup();
    let deadline = env.ledger().timestamp() + 86_400;
    client.init_campaign(&organizer, &token, &1_000, &deadline);
    // 5_000 then 2_000 is not ascending — must panic.
    client.set_stretch_goals(&vec![&env, 5_000_i128, 2_000_i128]);
}

// ── #309 – Reward fulfillment tracking ───────────────────────────────────────

#[test]
fn test_fulfill_reward_marks_backer_as_fulfilled() {
    let (env, _contract, client, token, organizer, contributor) = setup();
    let deadline = env.ledger().timestamp() + 86_400;
    client.init_campaign(&organizer, &token, &1_000, &deadline);

    StellarAssetClient::new(&env, &token).mint(&contributor, &1_000);
    client.contribute(&contributor, &1_000);

    assert!(!client.is_fulfilled(&contributor));
    client.fulfill_reward(&contributor);
    assert!(client.is_fulfilled(&contributor));
}

#[test]
#[should_panic]
fn test_fulfill_reward_twice_panics() {
    let (env, _contract, client, token, organizer, contributor) = setup();
    let deadline = env.ledger().timestamp() + 86_400;
    client.init_campaign(&organizer, &token, &1_000, &deadline);

    StellarAssetClient::new(&env, &token).mint(&contributor, &1_000);
    client.contribute(&contributor, &1_000);

    client.fulfill_reward(&contributor);
    client.fulfill_reward(&contributor); // must panic
}

#[test]
fn test_is_fulfilled_default_false() {
    let (env, _contract, client, token, organizer, contributor) = setup();
    let deadline = env.ledger().timestamp() + 86_400;
    client.init_campaign(&organizer, &token, &1_000, &deadline);

    assert!(!client.is_fulfilled(&contributor));
}

// ── #308 – Reward tiers ───────────────────────────────────────────────────────

#[test]
fn test_select_reward_tier_maps_pledge_to_tier() {
    let (env, _contract, client, token, organizer, contributor) = setup();
    let deadline = env.ledger().timestamp() + 86_400;
    client.init_campaign(&organizer, &token, &1_000, &deadline);

    client.set_reward_tiers(&soroban_sdk::vec![
        &env,
        RewardTier { min_pledge: 100, name: soroban_sdk::String::from_str(&env, "Basic") },
        RewardTier { min_pledge: 500, name: soroban_sdk::String::from_str(&env, "Premium") },
    ]);

    StellarAssetClient::new(&env, &token).mint(&contributor, &500);
    client.contribute(&contributor, &500);

    // Contributor has 500 — can select tier 1 (min 500).
    client.select_reward_tier(&contributor, &1);
    assert_eq!(client.get_selected_tier(&contributor), Some(1));
}

#[test]
fn test_select_reward_tier_can_be_updated() {
    let (env, _contract, client, token, organizer, contributor) = setup();
    let deadline = env.ledger().timestamp() + 86_400;
    client.init_campaign(&organizer, &token, &1_000, &deadline);

    client.set_reward_tiers(&soroban_sdk::vec![
        &env,
        RewardTier { min_pledge: 100, name: soroban_sdk::String::from_str(&env, "Basic") },
        RewardTier { min_pledge: 500, name: soroban_sdk::String::from_str(&env, "Premium") },
    ]);

    StellarAssetClient::new(&env, &token).mint(&contributor, &600);
    client.contribute(&contributor, &600);

    client.select_reward_tier(&contributor, &0);
    assert_eq!(client.get_selected_tier(&contributor), Some(0));

    // Upgrade to tier 1.
    client.select_reward_tier(&contributor, &1);
    assert_eq!(client.get_selected_tier(&contributor), Some(1));
}

#[test]
#[should_panic]
fn test_select_reward_tier_below_minimum_panics() {
    let (env, _contract, client, token, organizer, contributor) = setup();
    let deadline = env.ledger().timestamp() + 86_400;
    client.init_campaign(&organizer, &token, &1_000, &deadline);

    client.set_reward_tiers(&soroban_sdk::vec![
        &env,
        RewardTier { min_pledge: 500, name: soroban_sdk::String::from_str(&env, "Premium") },
    ]);

    StellarAssetClient::new(&env, &token).mint(&contributor, &100);
    client.contribute(&contributor, &100);

    // Only 100 pledged, tier requires 500 — must panic.
    client.select_reward_tier(&contributor, &0);
}

#[test]
#[should_panic]
fn test_select_invalid_tier_index_panics() {
    let (env, _contract, client, token, organizer, contributor) = setup();
    let deadline = env.ledger().timestamp() + 86_400;
    client.init_campaign(&organizer, &token, &1_000, &deadline);

    client.set_reward_tiers(&soroban_sdk::vec![
        &env,
        RewardTier { min_pledge: 100, name: soroban_sdk::String::from_str(&env, "Basic") },
    ]);

    StellarAssetClient::new(&env, &token).mint(&contributor, &500);
    client.contribute(&contributor, &500);

    // Tier index 5 doesn't exist — must panic.
    client.select_reward_tier(&contributor, &5);
}

#[test]
fn test_get_selected_tier_returns_none_before_selection() {
    let (env, _contract, client, token, organizer, contributor) = setup();
    let deadline = env.ledger().timestamp() + 86_400;
    client.init_campaign(&organizer, &token, &1_000, &deadline);

    assert_eq!(client.get_selected_tier(&contributor), None);
}
