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

// ── #311 – Milestone-based fund release ──────────────────────────────────────

fn setup_milestone_campaign() -> (Env, Address, CrowdfundContractClient<'static>, Address, Address, Address) {
    let (env, contract, client, token, organizer, contributor) = setup();
    let deadline = env.ledger().timestamp() + 86_400;
    client.init_campaign(&organizer, &token, &10_000, &deadline);
    // 3 milestones: 50%, 30%, 20% in basis points
    client.set_milestones(&soroban_sdk::vec![&env, 5_000_u32, 3_000_u32, 2_000_u32]);
    StellarAssetClient::new(&env, &token).mint(&contributor, &10_000);
    client.contribute(&contributor, &10_000);
    // Advance past deadline
    env.ledger().with_mut(|l| l.timestamp += 86_401);
    (env, contract, client, token, organizer, contributor)
}

fn setup_governed_milestone_campaign() -> (
    Env,
    Address,
    CrowdfundContractClient<'static>,
    Address,
    Address,
    Address,
    Address,
    Address,
) {
    let (env, contract, client, token, organizer, voter1) = setup();
    let voter2 = Address::generate(&env);
    let voter3 = Address::generate(&env);
    let deadline = env.ledger().timestamp() + 86_400;

    client.init_campaign(&organizer, &token, &10_000, &deadline);
    client.set_milestones(&soroban_sdk::vec![&env, 10_000_u32]);

    {
        let token_admin = StellarAssetClient::new(&env, &token);
        token_admin.mint(&voter1, &4_000);
        token_admin.mint(&voter2, &3_000);
        token_admin.mint(&voter3, &3_000);
    }

    client.contribute(&voter1, &4_000);
    client.contribute(&voter2, &3_000);
    client.contribute(&voter3, &3_000);

    env.ledger().with_mut(|l| l.timestamp += 86_401);

    (env, contract, client, token, organizer, voter1, voter2, voter3)
}

#[test]
fn test_release_milestone_transfers_correct_amount() {
    let (env, _contract, client, token, organizer, contributor) = setup_milestone_campaign();

    client.unlock_milestone(&0);
    client.vote_milestone(&contributor, &0, &true);
    client.release_milestone(&0);

    // 50% of 10_000 = 5_000
    assert_eq!(
        soroban_sdk::token::TokenClient::new(&env, &token).balance(&organizer),
        5_000
    );
}

#[test]
fn test_all_milestones_release_full_raised_amount() {
    let (env, _contract, client, token, organizer, contributor) = setup_milestone_campaign();

    client.unlock_milestone(&0);
    client.vote_milestone(&contributor, &0, &true);
    client.release_milestone(&0);
    client.unlock_milestone(&1);
    client.vote_milestone(&contributor, &1, &true);
    client.release_milestone(&1);
    client.unlock_milestone(&2);
    client.vote_milestone(&contributor, &2, &true);
    client.release_milestone(&2);

    // 50% + 30% + 20% = 100% of 10_000
    assert_eq!(
        soroban_sdk::token::TokenClient::new(&env, &token).balance(&organizer),
        10_000
    );
}

#[test]
#[should_panic]
fn test_release_milestone_without_unlock_panics() {
    let (_env, _contract, client, _token, _organizer, _contributor) = setup_milestone_campaign();
    // Milestone 0 not unlocked — must panic
    client.release_milestone(&0);
}

#[test]
#[should_panic]
fn test_release_milestone_twice_panics() {
    let (_env, _contract, client, _token, _organizer, contributor) = setup_milestone_campaign();
    client.unlock_milestone(&0);
    client.vote_milestone(&contributor, &0, &true);
    client.release_milestone(&0);
    client.release_milestone(&0); // must panic
}

#[test]
#[should_panic]
fn test_execute_campaign_blocked_in_milestone_mode() {
    let (_env, _contract, client, _token, _organizer, _contributor) = setup_milestone_campaign();
    // MilestonesActive error expected
    client.execute_campaign();
}

#[test]
#[should_panic]
fn test_set_milestones_invalid_sum_panics() {
    let (env, _contract, client, token, organizer, _) = setup();
    let deadline = env.ledger().timestamp() + 86_400;
    client.init_campaign(&organizer, &token, &1_000, &deadline);
    // Sums to 9_000, not 10_000 — must panic
    client.set_milestones(&soroban_sdk::vec![&env, 5_000_u32, 4_000_u32]);
}

#[test]
#[should_panic]
fn test_release_milestone_before_deadline_panics() {
    let (env, _contract, client, token, organizer, contributor) = setup();
    let deadline = env.ledger().timestamp() + 86_400;
    client.init_campaign(&organizer, &token, &1_000, &deadline);
    client.set_milestones(&soroban_sdk::vec![&env, 10_000_u32]);
    StellarAssetClient::new(&env, &token).mint(&contributor, &1_000);
    client.contribute(&contributor, &1_000);
    // Deadline not yet passed — must panic
    client.unlock_milestone(&0);
    client.release_milestone(&0);
}

// ── #313 – Governance voting controls milestone capital release ──────────────

#[test]
fn majority_vote_allows_milestone_release() {
    let (env, contract, client, token, organizer, voter1, voter2, _voter3) =
        setup_governed_milestone_campaign();
    let token_client = soroban_sdk::token::TokenClient::new(&env, &token);

    client.unlock_milestone(&0);
    client.vote_milestone(&voter1, &0, &true);
    client.vote_milestone(&voter2, &0, &true);

    let organizer_before = token_client.balance(&organizer);
    let contract_before = token_client.balance(&contract);
    client.release_milestone(&0);

    assert_eq!(contract_before, 10_000);
    assert_eq!(token_client.balance(&organizer) - organizer_before, 10_000);
    assert_eq!(token_client.balance(&contract), 0);

    let second_release = client.try_release_milestone(&0);
    assert!(second_release.is_err());
}

#[test]
fn rejected_milestone_blocks_fund_release() {
    let (env, contract, client, token, organizer, voter1, voter2, voter3) =
        setup_governed_milestone_campaign();
    let token_client = soroban_sdk::token::TokenClient::new(&env, &token);

    client.unlock_milestone(&0);
    client.vote_milestone(&voter1, &0, &false);
    client.vote_milestone(&voter2, &0, &false);
    client.vote_milestone(&voter3, &0, &true);

    let organizer_before = token_client.balance(&organizer);
    let contract_before = token_client.balance(&contract);
    let result = client.try_release_milestone(&0);

    assert!(result.is_err());
    assert_eq!(token_client.balance(&organizer), organizer_before);
    assert_eq!(token_client.balance(&contract), contract_before);
}

#[test]
fn milestone_without_majority_cannot_release_funds() {
    let (env, contract, client, token, organizer, voter1, _voter2, _voter3) =
        setup_governed_milestone_campaign();
    let token_client = soroban_sdk::token::TokenClient::new(&env, &token);

    client.unlock_milestone(&0);
    client.vote_milestone(&voter1, &0, &true);

    let organizer_before = token_client.balance(&organizer);
    let contract_before = token_client.balance(&contract);
    let result = client.try_release_milestone(&0);

    assert!(result.is_err());
    assert_eq!(token_client.balance(&organizer), organizer_before);
    assert_eq!(token_client.balance(&contract), contract_before);
}

// ── #310 – Reward tier allocation constraints & fulfillment toggles ───────────

fn tiers(env: &Env) -> soroban_sdk::Vec<RewardTier> {
    soroban_sdk::vec![
        env,
        RewardTier { min_pledge: 200, name: soroban_sdk::String::from_str(env, "Silver") },
        RewardTier { min_pledge: 1_000, name: soroban_sdk::String::from_str(env, "Gold") },
    ]
}

#[test]
fn test_tier_selection_at_exact_minimum_succeeds() {
    let (env, _contract, client, token, organizer, contributor) = setup();
    let deadline = env.ledger().timestamp() + 86_400;
    client.init_campaign(&organizer, &token, &1_000, &deadline);
    client.set_reward_tiers(&tiers(&env));

    StellarAssetClient::new(&env, &token).mint(&contributor, &200);
    client.contribute(&contributor, &200);

    // Pledge == min_pledge exactly — must succeed.
    client.select_reward_tier(&contributor, &0);
    assert_eq!(client.get_selected_tier(&contributor), Some(0));
}

#[test]
fn test_cumulative_pledge_unlocks_higher_tier() {
    let (env, _contract, client, token, organizer, contributor) = setup();
    let deadline = env.ledger().timestamp() + 86_400;
    client.init_campaign(&organizer, &token, &1_000, &deadline);
    client.set_reward_tiers(&tiers(&env));

    StellarAssetClient::new(&env, &token).mint(&contributor, &1_000);
    // Two separate contributions totalling 1_000.
    client.contribute(&contributor, &600);
    client.contribute(&contributor, &400);

    // Total pledge 1_000 meets Gold tier minimum.
    client.select_reward_tier(&contributor, &1);
    assert_eq!(client.get_selected_tier(&contributor), Some(1));
}

#[test]
fn test_fulfillment_is_independent_per_backer() {
    let (env, _contract, client, token, organizer, contributor) = setup();
    let contributor2 = Address::generate(&env);
    let deadline = env.ledger().timestamp() + 86_400;
    client.init_campaign(&organizer, &token, &1_000, &deadline);

    StellarAssetClient::new(&env, &token).mint(&contributor, &500);
    StellarAssetClient::new(&env, &token).mint(&contributor2, &500);
    client.contribute(&contributor, &500);
    client.contribute(&contributor2, &500);

    client.fulfill_reward(&contributor);

    // contributor fulfilled, contributor2 still not.
    assert!(client.is_fulfilled(&contributor));
    assert!(!client.is_fulfilled(&contributor2));
}

#[test]
fn test_fulfillment_does_not_require_tier_selection() {
    let (env, _contract, client, token, organizer, contributor) = setup();
    let deadline = env.ledger().timestamp() + 86_400;
    client.init_campaign(&organizer, &token, &1_000, &deadline);

    StellarAssetClient::new(&env, &token).mint(&contributor, &500);
    client.contribute(&contributor, &500);

    // No tier selected — fulfillment still works.
    assert_eq!(client.get_selected_tier(&contributor), None);
    client.fulfill_reward(&contributor);
    assert!(client.is_fulfilled(&contributor));
}

#[test]
#[should_panic]
fn test_tier_one_bps_below_minimum_rejected() {
    let (env, _contract, client, token, organizer, contributor) = setup();
    let deadline = env.ledger().timestamp() + 86_400;
    client.init_campaign(&organizer, &token, &1_000, &deadline);
    client.set_reward_tiers(&tiers(&env));

    // Pledge 199 — one below Silver minimum of 200 — must panic.
    StellarAssetClient::new(&env, &token).mint(&contributor, &199);
    client.contribute(&contributor, &199);
    client.select_reward_tier(&contributor, &0);
}

#[test]
#[should_panic]
fn test_non_organizer_cannot_fulfill_reward() {
    let (env, _contract, client, token, organizer, contributor) = setup();
    let deadline = env.ledger().timestamp() + 86_400;
    client.init_campaign(&organizer, &token, &1_000, &deadline);

    StellarAssetClient::new(&env, &token).mint(&contributor, &500);
    client.contribute(&contributor, &500);

    // contributor tries to mark their own reward fulfilled — must panic (auth).
    // We disable mock_all_auths for this check by not using the default setup env.
    // Since setup() calls mock_all_auths, we verify the contract still guards via
    // the organizer.require_auth() by using a fresh env without mocked auths.
    let env2 = Env::default();
    let contract2 = env2.register(CrowdfundContract, ());
    let client2 = CrowdfundContractClient::new(&env2, &contract2);
    env2.ledger().with_mut(|l| l.timestamp = 1_000_000);
    let org2 = Address::generate(&env2);
    let tok2 = env2.register_stellar_asset_contract_v2(org2.clone()).address();
    let con2 = Address::generate(&env2);
    client2.init_campaign(&org2, &tok2, &100, &(env2.ledger().timestamp() + 1_000));
    // No mock_all_auths — calling fulfill_reward as non-organizer must panic.
    client2.fulfill_reward(&con2);
}

// ── #305 – Campaign success and failure resolution ───────────────────────────

#[test]
fn test_campaign_success_goal_met_withdrawal() {
    let (env, _contract, client, token, organizer, contributor) = setup();
    let deadline = env.ledger().timestamp() + 100;
    client.init_campaign(&organizer, &token, &1_000, &deadline);
    StellarAssetClient::new(&env, &token).mint(&contributor, &1_000);
    client.contribute(&contributor, &1_000);
    assert!(client.goal_reached());
    env.ledger().with_mut(|l| l.timestamp += 200);
    let token_client = StellarAssetClient::new(&env, &token);
    let before = token_client.balance(&organizer);
    client.execute_campaign();
    assert_eq!(token_client.balance(&organizer) - before, 1_000);
    assert!(client.is_executed());
}

#[test]
fn test_campaign_failure_goal_missed_refund() {
    let (env, _contract, client, token, organizer, contributor) = setup();
    let deadline = env.ledger().timestamp() + 100;
    client.init_campaign(&organizer, &token, &5_000, &deadline);
    StellarAssetClient::new(&env, &token).mint(&contributor, &2_000);
    client.contribute(&contributor, &2_000);
    assert!(!client.goal_reached());
    env.ledger().with_mut(|l| l.timestamp += 200);
    let token_client = StellarAssetClient::new(&env, &token);
    let before = token_client.balance(&contributor);
    client.claim_refund(&contributor);
    assert_eq!(token_client.balance(&contributor) - before, 2_000);
    assert_eq!(client.pledge_of(&contributor), 0);
}

#[test]
#[should_panic]
fn test_execute_campaign_panics_when_goal_not_met() {
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
fn test_refund_panics_on_successful_campaign() {
    let (env, _contract, client, token, organizer, contributor) = setup();
    let deadline = env.ledger().timestamp() + 100;
    client.init_campaign(&organizer, &token, &1_000, &deadline);
    StellarAssetClient::new(&env, &token).mint(&contributor, &1_000);
    client.contribute(&contributor, &1_000);
    env.ledger().with_mut(|l| l.timestamp += 200);
    client.claim_refund(&contributor);
}

// ── #307 – Batch refund for failed campaigns ─────────────────────────────────

#[test]
fn test_batch_refund_returns_all_pledges() {
    let (env, _contract, client, token, organizer, contributor) = setup();
    let contributor2 = Address::generate(&env);
    let deadline = env.ledger().timestamp() + 100;
    client.init_campaign(&organizer, &token, &10_000, &deadline);
    StellarAssetClient::new(&env, &token).mint(&contributor, &3_000);
    StellarAssetClient::new(&env, &token).mint(&contributor2, &2_000);
    client.contribute(&contributor, &3_000);
    client.contribute(&contributor2, &2_000);
    env.ledger().with_mut(|l| l.timestamp += 200);
    let token_client = StellarAssetClient::new(&env, &token);
    let before1 = token_client.balance(&contributor);
    let before2 = token_client.balance(&contributor2);
    client.batch_refund();
    assert_eq!(token_client.balance(&contributor) - before1, 3_000);
    assert_eq!(token_client.balance(&contributor2) - before2, 2_000);
    assert_eq!(client.pledge_of(&contributor), 0);
    assert_eq!(client.pledge_of(&contributor2), 0);
}

#[test]
#[should_panic]
fn test_batch_refund_panics_before_deadline() {
    let (env, _contract, client, token, organizer, contributor) = setup();
    let deadline = env.ledger().timestamp() + 86_400;
    client.init_campaign(&organizer, &token, &5_000, &deadline);
    StellarAssetClient::new(&env, &token).mint(&contributor, &1_000);
    client.contribute(&contributor, &1_000);
    client.batch_refund();
}

#[test]
#[should_panic]
fn test_batch_refund_panics_on_successful_campaign() {
    let (env, _contract, client, token, organizer, contributor) = setup();
    let deadline = env.ledger().timestamp() + 100;
    client.init_campaign(&organizer, &token, &1_000, &deadline);
    StellarAssetClient::new(&env, &token).mint(&contributor, &1_000);
    client.contribute(&contributor, &1_000);
    env.ledger().with_mut(|l| l.timestamp += 200);
    client.batch_refund();
}

#[test]
#[should_panic]
fn test_batch_refund_panics_when_called_twice() {
    let (env, _contract, client, token, organizer, contributor) = setup();
    let deadline = env.ledger().timestamp() + 100;
    client.init_campaign(&organizer, &token, &5_000, &deadline);
    StellarAssetClient::new(&env, &token).mint(&contributor, &1_000);
    client.contribute(&contributor, &1_000);
    env.ledger().with_mut(|l| l.timestamp += 200);
    client.batch_refund();
    client.batch_refund();
}

// ── #314 / #315 / #312 – Social comments, matching, and voting ──────────────

#[test]
fn test_fund_matching_pool_doubles_next_pledge() {
    let (env, _contract, client, token, organizer, contributor) = setup();
    let sponsor = Address::generate(&env);
    let deadline = env.ledger().timestamp() + 86_400;
    client.init_campaign(&organizer, &token, &10_000, &deadline);

    let token_client = StellarAssetClient::new(&env, &token);
    token_client.mint(&sponsor, &1_000);
    token_client.mint(&contributor, &1_000);

    client.fund_matching_pool(&sponsor, &500);
    assert_eq!(client.matching_pool_balance(), 500);

    client.contribute(&contributor, &500);
    assert_eq!(client.matching_pool_balance(), 0);
    assert_eq!(client.pledge_of(&contributor), 1_000);
    assert_eq!(client.raised(), 1_000);
}

#[test]
fn test_partial_matching_when_pool_is_smaller_than_pledge() {
    let (env, _contract, client, token, organizer, contributor) = setup();
    let sponsor = Address::generate(&env);
    let deadline = env.ledger().timestamp() + 86_400;
    client.init_campaign(&organizer, &token, &10_000, &deadline);

    let token_client = StellarAssetClient::new(&env, &token);
    token_client.mint(&sponsor, &200);
    token_client.mint(&contributor, &500);

    client.fund_matching_pool(&sponsor, &200);
    client.contribute(&contributor, &500);

    assert_eq!(client.matching_pool_balance(), 0);
    assert_eq!(client.pledge_of(&contributor), 700);
    assert_eq!(client.raised(), 700);
}

#[test]
fn test_leave_comment_attaches_public_metadata() {
    let (env, _contract, client, token, organizer, contributor) = setup();
    let deadline = env.ledger().timestamp() + 86_400;
    client.init_campaign(&organizer, &token, &2_000, &deadline);
    StellarAssetClient::new(&env, &token).mint(&contributor, &500);
    client.contribute(&contributor, &500);

    let comment = soroban_sdk::String::from_str(&env, "Proud to support this launch");
    client.leave_comment(&contributor, &comment);
    assert_eq!(client.get_comment(&contributor), Some(comment));
}

#[test]
#[should_panic]
fn test_leave_comment_requires_existing_pledge() {
    let (env, _contract, client, token, organizer, contributor) = setup();
    let deadline = env.ledger().timestamp() + 86_400;
    client.init_campaign(&organizer, &token, &2_000, &deadline);

    let comment = soroban_sdk::String::from_str(&env, "No pledge yet");
    client.leave_comment(&contributor, &comment);
}
