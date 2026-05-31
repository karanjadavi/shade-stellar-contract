use soroban_sdk::{contracttype, contractevent, Address, String};

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SubRenewed {
    pub subscription_id: u64,
    pub plan_id: u64,
    pub customer: Address,
    pub timestamp: u64,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SubExpired {
    pub subscription_id: u64,
    pub plan_id: u64,
    pub customer: Address,
    pub timestamp: u64,
}

#[contracttype]
pub enum DataKey {
    Admin,
    AcceptedTokens,
    Plan(u64),
    PlanCount,
    Subscription(u64),
    SubscriptionCount,
}

/// A billing plan created by a merchant.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Plan {
    pub id: u64,
    pub merchant: Address,
    pub description: String,
    /// Token used for recurring billing.
    pub token: Address,
    /// Amount charged per interval (in token base units).
    pub amount: i128,
    /// Billing interval in seconds (e.g. 2_592_000 = 30 days).
    pub interval: u64,
    pub active: bool,
    pub created_at: u64,
    /// Seconds a subscription remains in `PastDue` before being terminated.
    /// `0` means no grace — a failed charge terminates immediately.
    pub grace_period: u64,
    pub creator: Option<Address>,
    pub trial_period: u64,
}

/// An active or cancelled subscription held by a customer.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Subscription {
    pub id: u64,
    pub plan_id: u64,
    pub customer: Address,
    pub status: SubscriptionStatus,
    pub created_at: u64,
    /// Timestamp of the last successful charge; 0 means never charged.
    pub last_charged: u64,
    /// Timestamp at which the subscription entered `PastDue`. `0` if not
    /// currently past due. Used to enforce the plan's grace period.
    pub past_due_since: u64,
    pub pending_downgrade_plan_id: u64,
}

#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum SubscriptionStatus {
    Active = 0,
    Cancelled = 1,
    /// Charge attempt failed; subscription is in the plan's grace window.
    /// Recovery via a fresh allowance restores it to Active.
    PastDue = 2,
    /// Grace window expired without payment; subscription is permanently
    /// terminated and can no longer be charged or recovered.
    Terminated = 3,
}

/// Outcome of a single billing cycle attempt. Returned by `process_charge`
/// and `process_billing_cycle` so callers (typically off-chain billing
/// bots) can react without parsing emitted events.
#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum ChargeOutcome {
    /// A charge was completed and `last_charged` updated.
    Charged = 0,
    /// The cycle hadn't elapsed yet — no state change.
    NotDueYet = 1,
    /// Charge failed for insufficient allowance and the subscription
    /// transitioned (or stayed) in `PastDue` within its grace window.
    EnteredGrace = 2,
    /// A previously past-due subscription recovered and was charged.
    Recovered = 3,
    /// Grace window expired without payment; subscription was terminated.
    Terminated = 4,
    /// Subscription was already cancelled or terminated when the cycle
    /// was processed — it is no longer chargeable. Only emitted by the
    /// batch [`process_billing_cycle`] flow, which tolerates terminal
    /// entries; the strict [`process_charge`] still panics in this case.
    Skipped = 5,
}
