#![cfg(test)]

use super::*;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, BytesN, Env};

fn register_campaign_ref(env: &Env, factory_id: &Address, organizer: Address, contract: Address) -> CampaignRef {
    env.as_contract(factory_id, || {
        let campaign_id = get_campaign_count(env) + 1;
        let deployed_at = env.ledger().timestamp();
        let campaign_ref = CampaignRef {
            campaign_id,
            contract,
            organizer,
            deployed_at,
        };
        env.storage()
            .persistent()
            .set(&DataKey::CampaignRef(campaign_id), &campaign_ref);
        env.storage()
            .persistent()
            .set(&DataKey::CampaignRefCount, &campaign_id);
        campaign_ref
    })
}

#[test]
fn test_deploy_campaign_tracks_active_protocols() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|ledger| ledger.timestamp = 1_000_000);

    let factory_id = env.register(CrowdfundFactory, ());
    let factory = CrowdfundFactoryClient::new(&env, &factory_id);
    let fake_wasm_hash = BytesN::from_array(&env, &[7u8; 32]);
    factory.initialize(&fake_wasm_hash);

    let organizer_a = Address::generate(&env);
    let organizer_b = Address::generate(&env);
    let campaign_a = register_campaign_ref(&env, &factory_id, organizer_a.clone(), Address::generate(&env));
    let campaign_b = register_campaign_ref(&env, &factory_id, organizer_b.clone(), Address::generate(&env));

    assert_eq!(factory.get_campaign_count(), 2);
    assert_eq!(campaign_a.campaign_id, 1);
    assert_eq!(campaign_b.campaign_id, 2);

    let campaigns = factory.get_all_campaigns();
    assert_eq!(campaigns.len(), 2);
    assert_eq!(campaigns.get_unchecked(0).organizer, organizer_a);
    assert_eq!(campaigns.get_unchecked(1).organizer, organizer_b);
}
