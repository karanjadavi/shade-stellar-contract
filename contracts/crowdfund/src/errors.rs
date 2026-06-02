use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum CrowdfundError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    InvalidGoal = 3,
    InvalidDeadline = 4,
    InvalidAmount = 5,
    CampaignEnded = 6,
    // Campaign has not yet passed its deadline.
    CampaignNotEnded = 7,
    // Goal was not reached; organizer cannot withdraw.
    GoalNotReached = 8,
    // Goal was reached; refunds are not available.
    GoalReached = 9,
    // Contributor has no recorded pledge to refund.
    NoPledge = 10,
    // Funds have already been withdrawn by the organizer.
    AlreadyExecuted = 11,
    // Reward for this backer has already been marked fulfilled.
    AlreadyFulfilled = 12,
    // Contributor's total pledge is below the selected tier's minimum.
    PledgeBelowTierMinimum = 13,
    // The supplied tier index does not exist.
    InvalidTier = 14,
}
