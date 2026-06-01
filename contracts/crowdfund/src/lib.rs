#![no_std]

mod errors;
#[cfg(test)]
mod test;

use errors::CrowdfundError;
use soroban_sdk::{
    contract, contractevent, contractimpl, contracttype, panic_with_error, token, vec, Address,
    Env, Vec,
};

#[contractevent]
pub struct CampaignExecutedEvent {
    pub amount: i128,
}

#[contractevent]
pub struct RefundClaimedEvent {
    pub contributor: Address,
    pub amount: i128,
}

#[contractevent]
pub struct StretchGoalReachedEvent {
    pub milestone_index: u32,
    pub threshold: i128,
}

#[contracttype]
enum DataKey {
    Organizer,
    Token,
    Goal,
    Deadline,
    Raised,
    // Tracks whether the campaign has been executed (funds withdrawn by organizer).
    Executed,
    // Stores per-contributor pledge amounts.
    Pledge(Address),
    // Ordered list of stretch goal thresholds.
    StretchGoals,
    // Tracks which stretch goal indexes have already been emitted.
    StretchTriggered(u32),
}

#[contract]
pub struct CrowdfundContract;

#[contractimpl]
impl CrowdfundContract {
    /// Initialise a campaign. Sets the funding goal (in token base units)
    /// and the deadline (Unix timestamp after which no contributions are
    /// accepted). Only callable once.
    ///
    /// # Arguments
    /// * `organizer` – address that will receive funds if the goal is met.
    /// * `token`     – accepted payment token.
    /// * `goal`      – target amount in token base units (must be > 0).
    /// * `deadline`  – Unix timestamp of the campaign end (must be in the future).
    pub fn init_campaign(
        env: Env,
        organizer: Address,
        token: Address,
        goal: i128,
        deadline: u64,
    ) {
        if env.storage().persistent().has(&DataKey::Organizer) {
            panic_with_error!(&env, CrowdfundError::AlreadyInitialized);
        }
        if goal <= 0 {
            panic_with_error!(&env, CrowdfundError::InvalidGoal);
        }
        if deadline <= env.ledger().timestamp() {
            panic_with_error!(&env, CrowdfundError::InvalidDeadline);
        }

        env.storage().persistent().set(&DataKey::Organizer, &organizer);
        env.storage().persistent().set(&DataKey::Token, &token);
        env.storage().persistent().set(&DataKey::Goal, &goal);
        env.storage().persistent().set(&DataKey::Deadline, &deadline);
        env.storage().persistent().set(&DataKey::Raised, &0_i128);
        env.storage().persistent().set(&DataKey::Executed, &false);
    }

    /// Contribute `amount` tokens to the campaign. The caller must have
    /// pre-approved the contract to spend at least `amount` from their
    /// balance. Panics after the deadline or if the campaign is not yet
    /// initialised.
    pub fn contribute(env: Env, contributor: Address, amount: i128) {
        contributor.require_auth();

        if amount <= 0 {
            panic_with_error!(&env, CrowdfundError::InvalidAmount);
        }

        let deadline: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::Deadline)
            .unwrap_or_else(|| panic_with_error!(&env, CrowdfundError::NotInitialized));

        if env.ledger().timestamp() > deadline {
            panic_with_error!(&env, CrowdfundError::CampaignEnded);
        }

        let token_addr: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Token)
            .unwrap_or_else(|| panic_with_error!(&env, CrowdfundError::NotInitialized));

        let contract_addr = env.current_contract_address();
        token::TokenClient::new(&env, &token_addr)
            .transfer(&contributor, &contract_addr, &amount);

        let raised: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::Raised)
            .unwrap_or(0);
        let new_raised = raised.saturating_add(amount);
        env.storage().persistent().set(&DataKey::Raised, &new_raised);

        // Record per-contributor pledge for potential refunds (#304).
        let prev_pledge: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::Pledge(contributor.clone()))
            .unwrap_or(0);
        env.storage()
            .persistent()
            .set(&DataKey::Pledge(contributor), &prev_pledge.saturating_add(amount));

        // Check and emit stretch goal events (#306).
        Self::check_stretch_goals(&env, new_raised);
    }

    /// Withdraw funds to the organizer after deadline if goal was met (#303).
    pub fn execute_campaign(env: Env) {
        let organizer: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Organizer)
            .unwrap_or_else(|| panic_with_error!(&env, CrowdfundError::NotInitialized));

        organizer.require_auth();

        let deadline: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::Deadline)
            .unwrap_or_else(|| panic_with_error!(&env, CrowdfundError::NotInitialized));

        if env.ledger().timestamp() <= deadline {
            panic_with_error!(&env, CrowdfundError::CampaignNotEnded);
        }

        let goal: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::Goal)
            .unwrap_or_else(|| panic_with_error!(&env, CrowdfundError::NotInitialized));

        let raised: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::Raised)
            .unwrap_or(0);

        if raised < goal {
            panic_with_error!(&env, CrowdfundError::GoalNotReached);
        }

        let executed: bool = env
            .storage()
            .persistent()
            .get(&DataKey::Executed)
            .unwrap_or(false);

        if executed {
            panic_with_error!(&env, CrowdfundError::AlreadyExecuted);
        }

        env.storage().persistent().set(&DataKey::Executed, &true);

        let token_addr: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Token)
            .unwrap_or_else(|| panic_with_error!(&env, CrowdfundError::NotInitialized));

        let contract_addr = env.current_contract_address();
        token::TokenClient::new(&env, &token_addr)
            .transfer(&contract_addr, &organizer, &raised);

        CampaignExecutedEvent { amount: raised }.publish(&env);
    }

    /// Allow a backer to reclaim their pledge after deadline if goal was not met (#304).
    pub fn claim_refund(env: Env, contributor: Address) {
        contributor.require_auth();

        let deadline: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::Deadline)
            .unwrap_or_else(|| panic_with_error!(&env, CrowdfundError::NotInitialized));

        if env.ledger().timestamp() <= deadline {
            panic_with_error!(&env, CrowdfundError::CampaignNotEnded);
        }

        let goal: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::Goal)
            .unwrap_or_else(|| panic_with_error!(&env, CrowdfundError::NotInitialized));

        let raised: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::Raised)
            .unwrap_or(0);

        if raised >= goal {
            panic_with_error!(&env, CrowdfundError::GoalReached);
        }

        let pledge: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::Pledge(contributor.clone()))
            .unwrap_or(0);

        if pledge == 0 {
            panic_with_error!(&env, CrowdfundError::NoPledge);
        }

        // Zero out pledge before transfer to prevent double-claim.
        env.storage()
            .persistent()
            .set(&DataKey::Pledge(contributor.clone()), &0_i128);

        let token_addr: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Token)
            .unwrap_or_else(|| panic_with_error!(&env, CrowdfundError::NotInitialized));

        let contract_addr = env.current_contract_address();
        token::TokenClient::new(&env, &token_addr)
            .transfer(&contract_addr, &contributor, &pledge);

        RefundClaimedEvent { contributor: contributor.clone(), amount: pledge }.publish(&env);
    }

    /// Add ordered stretch goal milestones (must be in ascending order, all > goal) (#306).
    /// Only the organizer can set these; must be called before deadline.
    pub fn set_stretch_goals(env: Env, milestones: Vec<i128>) {
        let organizer: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Organizer)
            .unwrap_or_else(|| panic_with_error!(&env, CrowdfundError::NotInitialized));

        organizer.require_auth();

        // Validate ascending order and all positive.
        let mut prev = 0_i128;
        for m in milestones.iter() {
            if m <= prev {
                panic_with_error!(&env, CrowdfundError::InvalidGoal);
            }
            prev = m;
        }

        env.storage()
            .persistent()
            .set(&DataKey::StretchGoals, &milestones);
    }

    /// Returns the pledge amount recorded for a given contributor.
    pub fn pledge_of(env: Env, contributor: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&DataKey::Pledge(contributor))
            .unwrap_or(0)
    }

    // ── Read-only accessors ───────────────────────────────────────────────────

    pub fn goal(env: Env) -> i128 {
        env.storage()
            .persistent()
            .get(&DataKey::Goal)
            .unwrap_or_else(|| panic_with_error!(&env, CrowdfundError::NotInitialized))
    }

    pub fn deadline(env: Env) -> u64 {
        env.storage()
            .persistent()
            .get(&DataKey::Deadline)
            .unwrap_or_else(|| panic_with_error!(&env, CrowdfundError::NotInitialized))
    }

    pub fn raised(env: Env) -> i128 {
        env.storage()
            .persistent()
            .get(&DataKey::Raised)
            .unwrap_or(0)
    }

    pub fn organizer(env: Env) -> Address {
        env.storage()
            .persistent()
            .get(&DataKey::Organizer)
            .unwrap_or_else(|| panic_with_error!(&env, CrowdfundError::NotInitialized))
    }

    pub fn is_executed(env: Env) -> bool {
        env.storage()
            .persistent()
            .get(&DataKey::Executed)
            .unwrap_or(false)
    }

    /// Returns `true` when the raised amount has reached or exceeded the goal.
    pub fn goal_reached(env: Env) -> bool {
        let goal: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::Goal)
            .unwrap_or_else(|| panic_with_error!(&env, CrowdfundError::NotInitialized));
        let raised: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::Raised)
            .unwrap_or(0);
        raised >= goal
    }

    // ── Internal helpers ──────────────────────────────────────────────────────

    /// Emit a `stretch / reached` event for each milestone crossed by `new_raised`
    /// that has not already been triggered.
    fn check_stretch_goals(env: &Env, new_raised: i128) {
        let milestones: Vec<i128> = env
            .storage()
            .persistent()
            .get(&DataKey::StretchGoals)
            .unwrap_or_else(|| vec![env]);

        for (idx, threshold) in milestones.iter().enumerate() {
            let idx_u32 = idx as u32;
            let already: bool = env
                .storage()
                .persistent()
                .get(&DataKey::StretchTriggered(idx_u32))
                .unwrap_or(false);

            if !already && new_raised >= threshold {
                env.storage()
                    .persistent()
                    .set(&DataKey::StretchTriggered(idx_u32), &true);
                StretchGoalReachedEvent {
                    milestone_index: idx_u32,
                    threshold,
                }
                .publish(env);
            }
        }
    }
}
