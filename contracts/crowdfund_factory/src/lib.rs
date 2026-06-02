#![no_std]

mod errors;
#[cfg(test)]
mod test;

use crate::errors::FactoryError;
use soroban_sdk::{
    contract, contractevent, contractimpl, contracttype, panic_with_error, Address, Bytes,
    BytesN, Env, IntoVal, Symbol, Vec,
};

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct CampaignRef {
    pub campaign_id: u64,
    pub contract: Address,
    pub organizer: Address,
    pub deployed_at: u64,
}

#[derive(Clone)]
#[contracttype]
enum DataKey {
    CrowdfundWasmHash,
    CampaignRef(u64),
    CampaignRefCount,
}

#[contractevent]
pub struct CampaignDeployedEvent {
    pub campaign_id: u64,
    pub contract: Address,
    pub organizer: Address,
    pub deployed_at: u64,
}

fn get_campaign_count(env: &Env) -> u64 {
    env.storage()
        .persistent()
        .get(&DataKey::CampaignRefCount)
        .unwrap_or(0)
}

#[contract]
pub struct CrowdfundFactory;

#[contractimpl]
impl CrowdfundFactory {
    pub fn initialize(env: Env, crowdfund_wasm_hash: BytesN<32>) {
        if env.storage().persistent().has(&DataKey::CrowdfundWasmHash) {
            panic_with_error!(&env, FactoryError::AlreadyInitialized);
        }
        env.storage()
            .persistent()
            .set(&DataKey::CrowdfundWasmHash, &crowdfund_wasm_hash);
        env.storage()
            .persistent()
            .set(&DataKey::CampaignRefCount, &0_u64);
    }

    pub fn set_crowdfund_wasm_hash(env: Env, crowdfund_wasm_hash: BytesN<32>) {
        if !env.storage().persistent().has(&DataKey::CrowdfundWasmHash) {
            panic_with_error!(&env, FactoryError::NotInitialized);
        }
        env.storage()
            .persistent()
            .set(&DataKey::CrowdfundWasmHash, &crowdfund_wasm_hash);
    }

    /// Deploy and initialize an independent crowdfund campaign (#316).
    pub fn deploy_campaign(
        env: Env,
        organizer: Address,
        token: Address,
        goal: i128,
        deadline: u64,
    ) -> CampaignRef {
        organizer.require_auth();

        let wasm_hash: BytesN<32> = env
            .storage()
            .persistent()
            .get(&DataKey::CrowdfundWasmHash)
            .unwrap_or_else(|| panic_with_error!(&env, FactoryError::WasmHashNotSet));

        let random: BytesN<32> = env.prng().gen();
        let salt = env
            .crypto()
            .keccak256(&Bytes::from_slice(&env, &random.to_array()));

        let campaign_addr = env.deployer().with_current_contract(salt).deploy_v2(wasm_hash, ());
        env.invoke_contract::<()>(
            &campaign_addr,
            &Symbol::new(&env, "init_campaign"),
            (organizer.clone(), token, goal, deadline).into_val(&env),
        );

        let campaign_id = get_campaign_count(&env) + 1;
        let deployed_at = env.ledger().timestamp();
        let campaign_ref = CampaignRef {
            campaign_id,
            contract: campaign_addr.clone(),
            organizer: organizer.clone(),
            deployed_at,
        };

        env.storage()
            .persistent()
            .set(&DataKey::CampaignRef(campaign_id), &campaign_ref);
        env.storage()
            .persistent()
            .set(&DataKey::CampaignRefCount, &campaign_id);

        CampaignDeployedEvent {
            campaign_id,
            contract: campaign_addr,
            organizer,
            deployed_at,
        }
        .publish(&env);

        campaign_ref
    }

    pub fn get_campaign_ref(env: Env, campaign_id: u64) -> CampaignRef {
        env.storage()
            .persistent()
            .get(&DataKey::CampaignRef(campaign_id))
            .unwrap_or_else(|| panic_with_error!(&env, FactoryError::CampaignNotFound))
    }

    pub fn get_campaign_count(env: Env) -> u64 {
        get_campaign_count(&env)
    }

    pub fn get_all_campaigns(env: Env) -> Vec<CampaignRef> {
        let count = get_campaign_count(&env);
        let mut campaigns = Vec::new(&env);
        for i in 1..=count {
            if let Some(campaign_ref) = env.storage().persistent().get(&DataKey::CampaignRef(i)) {
                campaigns.push_back(campaign_ref);
            }
        }
        campaigns
    }
}
