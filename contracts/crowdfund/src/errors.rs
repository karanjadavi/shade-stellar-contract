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
    // No milestones have been set on this campaign.
    MilestonesNotSet = 15,
    // This milestone has already been released.
    MilestoneAlreadyReleased = 16,
    // This milestone has not yet been unlocked by the organizer.
    MilestoneNotUnlocked = 17,
    // Milestone percentages must be non-zero, and sum to exactly 10 000 bps (100 %).
    InvalidMilestonePercentages = 18,
    // Campaign is in milestone mode; use release_milestone instead of execute_campaign.
    MilestonesActive = 19,
    // Milestone release does not have a strict majority approval from backers.
    MilestoneNotApproved = 20,
    // Only contributors with a recorded pledge can vote on milestone release.
    NotBacker = 21,
    // A backer can only vote once per milestone.
    MilestoneVoteAlreadyCast = 22,
    // Shade payment gateway address has not been configured.
    ShadeGatewayNotSet = 23,
    // Merchant account address has not been configured.
    MerchantAccountNotSet = 24,
    // Batch refund has already been processed.
    RefundAlreadyProcessed = 25,
    // Sponsor matching pool cannot satisfy a match request.
    InsufficientMatchingPool = 26,
    // Pledge comment exceeds the configured maximum length.
    CommentTooLong = 27,
}
