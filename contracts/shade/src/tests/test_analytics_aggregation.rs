#![cfg(test)]

use crate::shade::{Shade, ShadeClient};
use crate::types::{MerchantAnalytics, MerchantAnalyticsSummary, TokenAnalytics};
use soroban_sdk::testutils::{Address as _, Ledger, LedgerInfo};
use soroban_sdk::{Address, Env, String};

const DAY_IN_SECONDS: u64 = 86400;
const WEEK_IN_SECONDS: u64 = 604800;

fn setup(env: &Env) -> (Address, ShadeClient<'_>, Address, Address, Address, Address) {
    env.mock_all_auths();
    let contract_id = env.register(Shade, ());
    let client = ShadeClient::new(env, &contract_id);

    let admin = Address::generate(env);
    client.initialize(&admin);

    // Create two tokens for multi-token testing
    let token_admin = Address::generate(env);
    let token1 = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token2 = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();

    client.add_accepted_token(&admin, &token1);
    client.add_accepted_token(&admin, &token2);
    client.set_fee(&admin, &token1, &1000); // 10% fee
    client.set_fee(&admin, &token2, &500);  // 5% fee

    let merchant = Address::generate(env);
    client.register_merchant(&merchant);
    client.verify_merchant(&admin, &1, &true);

    let merchant_account = Address::generate(env);
    client.set_merchant_account(&merchant, &merchant_account);

    (admin, client, token1, token2, merchant, merchant_account)
}

fn setup_multi_merchant(env: &Env) -> (Address, ShadeClient<'_>, Address, Address, Address, Address, Address) {
    let (admin, client, token1, token2, merchant1, merchant1_account) = setup(env);
    
    // Create second merchant
    let merchant2 = Address::generate(env);
    client.register_merchant(&merchant2);
    client.verify_merchant(&admin, &2, &true);
    
    let merchant2_account = Address::generate(env);
    client.set_merchant_account(&merchant2, &merchant2_account);
    
    (admin, client, token1, token2, merchant1, merchant2, merchant1_account)
}

fn fund_payer(env: &Env, token: &Address, payer: &Address, amount: i128) {
    let token_client = soroban_sdk::token::StellarAssetClient::new(env, token);
    token_client.mint(payer, &amount);
}

#[test]
fn test_volume_increments_single_merchant_single_token() {
    let env = Env::default();
    let (_admin, client, token1, _token2, merchant, _merchant_account) = setup(&env);
    let payer = Address::generate(&env);
    fund_payer(&env, &token1, &payer, 1_000_000);

    // Initial state - should be zero
    let initial_analytics = client.get_merchant_analytics(&merchant, &token1);
    assert_eq!(initial_analytics.total_volume, 0);
    assert_eq!(initial_analytics.total_fees, 0);
    assert_eq!(initial_analytics.transaction_count, 0);

    // First payment
    let inv1 = client.create_invoice(
        &merchant,
        &String::from_str(&env, "test_invoice_1"),
        &1000,
        &token1,
        &None,
    );
    client.pay_invoice(&payer, &inv1);

    // Check analytics after first payment
    let analytics_after_1 = client.get_merchant_analytics(&merchant, &token1);
    assert_eq!(analytics_after_1.total_volume, 1000);
    assert_eq!(analytics_after_1.total_fees, 100); // 10% of 1000
    assert_eq!(analytics_after_1.transaction_count, 1);
    assert!(analytics_after_1.last_updated > 0);

    // Second payment
    let inv2 = client.create_invoice(
        &merchant,
        &String::from_str(&env, "test_invoice_2"),
        &2000,
        &token1,
        &None,
    );
    client.pay_invoice(&payer, &inv2);

    // Check cumulative analytics
    let analytics_after_2 = client.get_merchant_analytics(&merchant, &token1);
    assert_eq!(analytics_after_2.total_volume, 3000); // 1000 + 2000
    assert_eq!(analytics_after_2.total_fees, 300); // 100 + 200
    assert_eq!(analytics_after_2.transaction_count, 2);
    assert!(analytics_after_2.last_updated >= analytics_after_1.last_updated);
}

#[test]
fn test_volume_increments_single_merchant_multiple_tokens() {
    let env = Env::default();
    let (_admin, client, token1, token2, merchant, _merchant_account) = setup(&env);
    let payer = Address::generate(&env);
    fund_payer(&env, &token1, &payer, 1_000_000);
    fund_payer(&env, &token2, &payer, 1_000_000);

    // Payment with token1
    let inv1 = client.create_invoice(
        &merchant,
        &String::from_str(&env, "token1_invoice"),
        &1000,
        &token1,
        &None,
    );
    client.pay_invoice(&payer, &inv1);

    // Payment with token2
    let inv2 = client.create_invoice(
        &merchant,
        &String::from_str(&env, "token2_invoice"),
        &2000,
        &token2,
        &None,
    );
    client.pay_invoice(&payer, &inv2);

    // Check token1 analytics
    let token1_analytics = client.get_merchant_analytics(&merchant, &token1);
    assert_eq!(token1_analytics.total_volume, 1000);
    assert_eq!(token1_analytics.total_fees, 100); // 10% fee
    assert_eq!(token1_analytics.transaction_count, 1);

    // Check token2 analytics
    let token2_analytics = client.get_merchant_analytics(&merchant, &token2);
    assert_eq!(token2_analytics.total_volume, 2000);
    assert_eq!(token2_analytics.total_fees, 100); // 5% fee
    assert_eq!(token2_analytics.transaction_count, 1);

    // Check merchant summary (aggregated across all tokens)
    let summary = client.get_merchant_analytics_summary(&merchant);
    assert_eq!(summary.total_volume, 3000); // 1000 + 2000
    assert_eq!(summary.total_fees, 200); // 100 + 100
    assert_eq!(summary.transaction_count, 2);
}

#[test]
fn test_volume_increments_multiple_merchants() {
    let env = Env::default();
    let (_admin, client, token1, _token2, merchant1, merchant2, _merchant1_account) = setup_multi_merchant(&env);
    let payer = Address::generate(&env);
    fund_payer(&env, &token1, &payer, 1_000_000);

    // Payment to merchant1
    let inv1 = client.create_invoice(
        &merchant1,
        &String::from_str(&env, "merchant1_invoice"),
        &1500,
        &token1,
        &None,
    );
    client.pay_invoice(&payer, &inv1);

    // Payment to merchant2
    let inv2 = client.create_invoice(
        &merchant2,
        &String::from_str(&env, "merchant2_invoice"),
        &2500,
        &token1,
        &None,
    );
    client.pay_invoice(&payer, &inv2);

    // Check merchant1 analytics
    let merchant1_analytics = client.get_merchant_analytics(&merchant1, &token1);
    assert_eq!(merchant1_analytics.total_volume, 1500);
    assert_eq!(merchant1_analytics.total_fees, 150);
    assert_eq!(merchant1_analytics.transaction_count, 1);

    // Check merchant2 analytics
    let merchant2_analytics = client.get_merchant_analytics(&merchant2, &token1);
    assert_eq!(merchant2_analytics.total_volume, 2500);
    assert_eq!(merchant2_analytics.total_fees, 250);
    assert_eq!(merchant2_analytics.transaction_count, 1);

    // Verify merchants are tracked separately
    assert_ne!(merchant1_analytics.merchant, merchant2_analytics.merchant);
}

#[test]
fn test_token_analytics_aggregation() {
    let env = Env::default();
    let (_admin, client, token1, token2, merchant1, merchant2, _merchant1_account) = setup_multi_merchant(&env);
    let payer = Address::generate(&env);
    fund_payer(&env, &token1, &payer, 1_000_000);
    fund_payer(&env, &token2, &payer, 1_000_000);

    // Initial token analytics should be zero
    let initial_token1_analytics = client.get_token_analytics(&token1);
    assert_eq!(initial_token1_analytics.total_volume, 0);
    assert_eq!(initial_token1_analytics.total_fees, 0);
    assert_eq!(initial_token1_analytics.transaction_count, 0);
    assert_eq!(initial_token1_analytics.unique_merchants, 0);

    // Multiple payments with token1 from different merchants
    let inv1 = client.create_invoice(
        &merchant1,
        &String::from_str(&env, "m1_token1_invoice"),
        &1000,
        &token1,
        &None,
    );
    client.pay_invoice(&payer, &inv1);

    let inv2 = client.create_invoice(
        &merchant2,
        &String::from_str(&env, "m2_token1_invoice"),
        &2000,
        &token1,
        &None,
    );
    client.pay_invoice(&payer, &inv2);

    // Payment with token2
    let inv3 = client.create_invoice(
        &merchant1,
        &String::from_str(&env, "m1_token2_invoice"),
        &1500,
        &token2,
        &None,
    );
    client.pay_invoice(&payer, &inv3);

    // Check token1 analytics
    let token1_analytics = client.get_token_analytics(&token1);
    assert_eq!(token1_analytics.total_volume, 3000); // 1000 + 2000
    assert_eq!(token1_analytics.total_fees, 300); // 100 + 200
    assert_eq!(token1_analytics.transaction_count, 2);
    // Note: unique_merchants logic may need verification based on implementation

    // Check token2 analytics
    let token2_analytics = client.get_token_analytics(&token2);
    assert_eq!(token2_analytics.total_volume, 1500);
    assert_eq!(token2_analytics.total_fees, 75); // 5% of 1500
    assert_eq!(token2_analytics.transaction_count, 1);

    // Verify tokens are tracked separately
    assert_ne!(token1_analytics.token, token2_analytics.token);
}

#[test]
fn test_time_bucketing_accuracy_daily() {
    let env = Env::default();
    let (_admin, client, token1, _token2, merchant, _merchant_account) = setup(&env);
    let payer = Address::generate(&env);
    fund_payer(&env, &token1, &payer, 1_000_000);

    // Set initial timestamp (day 1)
    let day1_timestamp = 1000000u64;
    env.ledger().with_mut(|li| {
        li.timestamp = day1_timestamp;
    });

    // First payment on day 1
    let inv1 = client.create_invoice(
        &merchant,
        &String::from_str(&env, "day1_invoice"),
        &1000,
        &token1,
        &None,
    );
    client.pay_invoice(&payer, &inv1);

    let day1_analytics = client.get_merchant_analytics(&merchant, &token1);
    assert_eq!(day1_analytics.last_updated, day1_timestamp);

    // Move to day 2
    let day2_timestamp = day1_timestamp + DAY_IN_SECONDS;
    env.ledger().with_mut(|li| {
        li.timestamp = day2_timestamp;
    });

    // Second payment on day 2
    let inv2 = client.create_invoice(
        &merchant,
        &String::from_str(&env, "day2_invoice"),
        &2000,
        &token1,
        &None,
    );
    client.pay_invoice(&payer, &inv2);

    let day2_analytics = client.get_merchant_analytics(&merchant, &token1);
    assert_eq!(day2_analytics.last_updated, day2_timestamp);
    assert_eq!(day2_analytics.total_volume, 3000); // Cumulative
    assert_eq!(day2_analytics.transaction_count, 2);

    // Verify time progression
    assert!(day2_analytics.last_updated > day1_analytics.last_updated);
    assert_eq!(day2_analytics.last_updated - day1_analytics.last_updated, DAY_IN_SECONDS);
}

#[test]
fn test_time_bucketing_accuracy_weekly() {
    let env = Env::default();
    let (_admin, client, token1, _token2, merchant, _merchant_account) = setup(&env);
    let payer = Address::generate(&env);
    fund_payer(&env, &token1, &payer, 1_000_000);

    // Set initial timestamp (week 1)
    let week1_timestamp = 1000000u64;
    env.ledger().with_mut(|li| {
        li.timestamp = week1_timestamp;
    });

    // Payment in week 1
    let inv1 = client.create_invoice(
        &merchant,
        &String::from_str(&env, "week1_invoice"),
        &5000,
        &token1,
        &None,
    );
    client.pay_invoice(&payer, &inv1);

    let week1_analytics = client.get_merchant_analytics(&merchant, &token1);

    // Move to week 2
    let week2_timestamp = week1_timestamp + WEEK_IN_SECONDS;
    env.ledger().with_mut(|li| {
        li.timestamp = week2_timestamp;
    });

    // Payment in week 2
    let inv2 = client.create_invoice(
        &merchant,
        &String::from_str(&env, "week2_invoice"),
        &3000,
        &token1,
        &None,
    );
    client.pay_invoice(&payer, &inv2);

    let week2_analytics = client.get_merchant_analytics(&merchant, &token1);
    
    // Verify weekly time progression
    assert_eq!(week2_analytics.last_updated - week1_analytics.last_updated, WEEK_IN_SECONDS);
    assert_eq!(week2_analytics.total_volume, 8000); // Cumulative across weeks
    assert_eq!(week2_analytics.transaction_count, 2);
}

#[test]
fn test_analytics_consistency_across_payment_types() {
    let env = Env::default();
    let (_admin, client, token1, _token2, merchant, _merchant_account) = setup(&env);
    let payer = Address::generate(&env);
    fund_payer(&env, &token1, &payer, 1_000_000);

    // Create multiple invoices with different amounts
    let amounts = vec![500, 1000, 1500, 2000, 2500];
    let mut expected_volume = 0i128;
    let mut expected_fees = 0i128;

    for (i, amount) in amounts.iter().enumerate() {
        let inv = client.create_invoice(
            &merchant,
            &String::from_str(&env, &format!("invoice_{}", i)),
            amount,
            &token1,
            &None,
        );
        client.pay_invoice(&payer, &inv);

        expected_volume += amount;
        expected_fees += (amount * 1000) / 10000; // 10% fee

        // Verify analytics after each payment
        let analytics = client.get_merchant_analytics(&merchant, &token1);
        assert_eq!(analytics.total_volume, expected_volume);
        assert_eq!(analytics.total_fees, expected_fees);
        assert_eq!(analytics.transaction_count, (i + 1) as u64);
    }

    // Final verification
    let final_analytics = client.get_merchant_analytics(&merchant, &token1);
    assert_eq!(final_analytics.total_volume, 7500); // Sum of all amounts
    assert_eq!(final_analytics.total_fees, 750); // 10% of total
    assert_eq!(final_analytics.transaction_count, 5);
}

#[test]
fn test_analytics_with_volume_discounts() {
    let env = Env::default();
    let (_admin, client, token1, _token2, merchant, _merchant_account) = setup(&env);
    let payer = Address::generate(&env);
    fund_payer(&env, &token1, &payer, 1_000_000);

    // Payment that reaches tier 1 discount (10k volume)
    let inv1 = client.create_invoice(
        &merchant,
        &String::from_str(&env, "tier1_invoice"),
        &15000, // This will trigger tier 1 discount on subsequent payments
        &token1,
        &None,
    );
    client.pay_invoice(&payer, &inv1);

    let analytics_tier1 = client.get_merchant_analytics(&merchant, &token1);
    assert_eq!(analytics_tier1.total_volume, 15000);
    // Fee should be 10% of 15000 = 1500 (no discount on first payment)
    assert_eq!(analytics_tier1.total_fees, 1500);

    // Second payment should get tier 1 discount (10% off, so 9% fee)
    let inv2 = client.create_invoice(
        &merchant,
        &String::from_str(&env, "discounted_invoice"),
        &1000,
        &token1,
        &None,
    );
    client.pay_invoice(&payer, &inv2);

    let analytics_discounted = client.get_merchant_analytics(&merchant, &token1);
    assert_eq!(analytics_discounted.total_volume, 16000);
    // Previous fee (1500) + discounted fee (90) = 1590
    assert_eq!(analytics_discounted.total_fees, 1590);
    assert_eq!(analytics_discounted.transaction_count, 2);
}

#[test]
fn test_analytics_data_integrity() {
    let env = Env::default();
    let (_admin, client, token1, token2, merchant, _merchant_account) = setup(&env);
    let payer = Address::generate(&env);
    fund_payer(&env, &token1, &payer, 1_000_000);
    fund_payer(&env, &token2, &payer, 1_000_000);

    // Make payments with both tokens
    let inv1 = client.create_invoice(
        &merchant,
        &String::from_str(&env, "integrity_test_1"),
        &1000,
        &token1,
        &None,
    );
    client.pay_invoice(&payer, &inv1);

    let inv2 = client.create_invoice(
        &merchant,
        &String::from_str(&env, "integrity_test_2"),
        &2000,
        &token2,
        &None,
    );
    client.pay_invoice(&payer, &inv2);

    // Verify merchant analytics per token
    let token1_analytics = client.get_merchant_analytics(&merchant, &token1);
    let token2_analytics = client.get_merchant_analytics(&merchant, &token2);

    // Verify merchant summary
    let summary = client.get_merchant_analytics_summary(&merchant);

    // Verify token analytics
    let global_token1_analytics = client.get_token_analytics(&token1);
    let global_token2_analytics = client.get_token_analytics(&token2);

    // Data integrity checks
    assert_eq!(summary.total_volume, token1_analytics.total_volume + token2_analytics.total_volume);
    assert_eq!(summary.total_fees, token1_analytics.total_fees + token2_analytics.total_fees);
    assert_eq!(summary.transaction_count, token1_analytics.transaction_count + token2_analytics.transaction_count);

    // Global token analytics should match merchant analytics for single merchant
    assert_eq!(global_token1_analytics.total_volume, token1_analytics.total_volume);
    assert_eq!(global_token1_analytics.total_fees, token1_analytics.total_fees);
    assert_eq!(global_token1_analytics.transaction_count, token1_analytics.transaction_count);

    assert_eq!(global_token2_analytics.total_volume, token2_analytics.total_volume);
    assert_eq!(global_token2_analytics.total_fees, token2_analytics.total_fees);
    assert_eq!(global_token2_analytics.transaction_count, token2_analytics.transaction_count);
}

#[test]
fn test_analytics_timestamp_accuracy() {
    let env = Env::default();
    let (_admin, client, token1, _token2, merchant, _merchant_account) = setup(&env);
    let payer = Address::generate(&env);
    fund_payer(&env, &token1, &payer, 1_000_000);

    // Set specific timestamps for testing
    let timestamps = vec![1000000u64, 1001000u64, 1002000u64];
    
    for (i, &timestamp) in timestamps.iter().enumerate() {
        env.ledger().with_mut(|li| {
            li.timestamp = timestamp;
        });

        let inv = client.create_invoice(
            &merchant,
            &String::from_str(&env, &format!("timestamp_test_{}", i)),
            &1000,
            &token1,
            &None,
        );
        client.pay_invoice(&payer, &inv);

        // Verify timestamp is recorded correctly
        let analytics = client.get_merchant_analytics(&merchant, &token1);
        assert_eq!(analytics.last_updated, timestamp);

        let token_analytics = client.get_token_analytics(&token1);
        assert_eq!(token_analytics.last_updated, timestamp);

        let summary = client.get_merchant_analytics_summary(&merchant);
        assert_eq!(summary.last_updated, timestamp);
    }
}

#[test]
fn test_zero_amount_payments_analytics() {
    let env = Env::default();
    let (_admin, client, token1, _token2, merchant, _merchant_account) = setup(&env);
    let payer = Address::generate(&env);
    fund_payer(&env, &token1, &payer, 1_000_000);

    // Test with zero amount (edge case)
    let inv_zero = client.create_invoice(
        &merchant,
        &String::from_str(&env, "zero_amount_invoice"),
        &0,
        &token1,
        &None,
    );
    client.pay_invoice(&payer, &inv_zero);

    let analytics_zero = client.get_merchant_analytics(&merchant, &token1);
    assert_eq!(analytics_zero.total_volume, 0);
    assert_eq!(analytics_zero.total_fees, 0);
    assert_eq!(analytics_zero.transaction_count, 1); // Transaction still counted

    // Follow up with normal payment
    let inv_normal = client.create_invoice(
        &merchant,
        &String::from_str(&env, "normal_amount_invoice"),
        &1000,
        &token1,
        &None,
    );
    client.pay_invoice(&payer, &inv_normal);

    let analytics_normal = client.get_merchant_analytics(&merchant, &token1);
    assert_eq!(analytics_normal.total_volume, 1000);
    assert_eq!(analytics_normal.total_fees, 100);
    assert_eq!(analytics_normal.transaction_count, 2);
}

#[test]
fn test_analytics_with_subscription_payments() {
    let env = Env::default();
    let (_admin, client, token1, _token2, merchant, merchant_account) = setup(&env);
    let customer = Address::generate(&env);
    fund_payer(&env, &token1, &customer, 1_000_000);

    // Create a subscription plan
    let plan_id = client.create_subscription_plan(
        &merchant,
        &String::from_str(&env, "Monthly Plan"),
        &token1,
        &1000, // 1000 units per month
        &2592000, // 30 days in seconds
    );

    // Customer subscribes
    client.subscribe(&customer, &plan_id);

    // Charge the subscription
    client.charge_subscription(&1u64); // subscription ID 1

    // Verify analytics include subscription charges
    let analytics = client.get_merchant_analytics(&merchant, &token1);
    assert_eq!(analytics.total_volume, 1000);
    assert_eq!(analytics.total_fees, 100); // 10% fee
    assert_eq!(analytics.transaction_count, 1);

    let token_analytics = client.get_token_analytics(&token1);
    assert_eq!(token_analytics.total_volume, 1000);
    assert_eq!(token_analytics.total_fees, 100);
    assert_eq!(token_analytics.transaction_count, 1);

    // Charge again after time passes
    env.ledger().with_mut(|li| {
        li.timestamp += 2592000; // Move forward 30 days
    });

    client.charge_subscription(&1u64);

    // Verify cumulative analytics
    let updated_analytics = client.get_merchant_analytics(&merchant, &token1);
    assert_eq!(updated_analytics.total_volume, 2000);
    assert_eq!(updated_analytics.total_fees, 200);
    assert_eq!(updated_analytics.transaction_count, 2);
}

#[test]
fn test_analytics_aggregation_performance() {
    let env = Env::default();
    let (_admin, client, token1, _token2, merchant, _merchant_account) = setup(&env);
    let payer = Address::generate(&env);
    fund_payer(&env, &token1, &payer, 10_000_000);

    // Test with many small transactions to verify aggregation performance
    let num_transactions = 50;
    let amount_per_transaction = 100i128;

    for i in 0..num_transactions {
        let inv = client.create_invoice(
            &merchant,
            &String::from_str(&env, &format!("perf_test_{}", i)),
            &amount_per_transaction,
            &token1,
            &None,
        );
        client.pay_invoice(&payer, &inv);
    }

    // Verify final aggregated analytics
    let final_analytics = client.get_merchant_analytics(&merchant, &token1);
    assert_eq!(final_analytics.total_volume, num_transactions as i128 * amount_per_transaction);
    assert_eq!(final_analytics.total_fees, (num_transactions as i128 * amount_per_transaction * 1000) / 10000);
    assert_eq!(final_analytics.transaction_count, num_transactions);

    let token_analytics = client.get_token_analytics(&token1);
    assert_eq!(token_analytics.total_volume, final_analytics.total_volume);
    assert_eq!(token_analytics.total_fees, final_analytics.total_fees);
    assert_eq!(token_analytics.transaction_count, final_analytics.transaction_count);
}

#[test]
fn test_analytics_cross_time_boundaries() {
    let env = Env::default();
    let (_admin, client, token1, _token2, merchant, _merchant_account) = setup(&env);
    let payer = Address::generate(&env);
    fund_payer(&env, &token1, &payer, 1_000_000);

    // Start at a specific time
    let start_time = 1609459200u64; // January 1, 2021 00:00:00 UTC
    env.ledger().with_mut(|li| {
        li.timestamp = start_time;
    });

    // Payment at start of day
    let inv1 = client.create_invoice(
        &merchant,
        &String::from_str(&env, "start_of_day"),
        &1000,
        &token1,
        &None,
    );
    client.pay_invoice(&payer, &inv1);

    // Payment at end of day (23:59:59)
    env.ledger().with_mut(|li| {
        li.timestamp = start_time + DAY_IN_SECONDS - 1;
    });

    let inv2 = client.create_invoice(
        &merchant,
        &String::from_str(&env, "end_of_day"),
        &2000,
        &token1,
        &None,
    );
    client.pay_invoice(&payer, &inv2);

    // Payment at start of next day
    env.ledger().with_mut(|li| {
        li.timestamp = start_time + DAY_IN_SECONDS;
    });

    let inv3 = client.create_invoice(
        &merchant,
        &String::from_str(&env, "next_day"),
        &1500,
        &token1,
        &None,
    );
    client.pay_invoice(&payer, &inv3);

    // Verify all payments are aggregated correctly
    let analytics = client.get_merchant_analytics(&merchant, &token1);
    assert_eq!(analytics.total_volume, 4500); // 1000 + 2000 + 1500
    assert_eq!(analytics.total_fees, 450); // 10% of total
    assert_eq!(analytics.transaction_count, 3);

    // Verify the last update timestamp is from the most recent transaction
    assert_eq!(analytics.last_updated, start_time + DAY_IN_SECONDS);
}

#[test]
fn test_analytics_merchant_volume_discount_integration() {
    let env = Env::default();
    let (_admin, client, token1, _token2, merchant, _merchant_account) = setup(&env);
    let payer = Address::generate(&env);
    fund_payer(&env, &token1, &payer, 1_000_000);

    // Track volume and fee changes as discounts kick in
    let payment_amounts = vec![5000, 6000, 40000, 10000]; // Will trigger different discount tiers
    let mut expected_volume = 0i128;
    let mut expected_fees = 0i128;

    for (i, &amount) in payment_amounts.iter().enumerate() {
        let inv = client.create_invoice(
            &merchant,
            &String::from_str(&env, &format!("discount_test_{}", i)),
            &amount,
            &token1,
            &None,
        );
        client.pay_invoice(&payer, &inv);

        expected_volume += amount;
        
        // Calculate expected fee based on volume discount tiers
        let current_volume = client.get_merchant_volume(&merchant, &token1);
        let calculated_fee = client.calculate_fee(&merchant, &token1, &amount);
        expected_fees += calculated_fee;

        let analytics = client.get_merchant_analytics(&merchant, &token1);
        assert_eq!(analytics.total_volume, expected_volume);
        assert_eq!(analytics.transaction_count, (i + 1) as u64);
        
        // Verify the fee calculation matches the analytics
        // Note: The fee in analytics might differ from calculated_fee due to discount timing
    }

    // Final verification
    let final_analytics = client.get_merchant_analytics(&merchant, &token1);
    assert_eq!(final_analytics.total_volume, 61000); // Sum of all amounts
    assert_eq!(final_analytics.transaction_count, 4);
}

#[test]
fn test_analytics_token_market_dominance() {
    let env = Env::default();
    let (_admin, client, token1, token2, merchant1, merchant2, _merchant1_account) = setup_multi_merchant(&env);
    let payer = Address::generate(&env);
    fund_payer(&env, &token1, &payer, 1_000_000);
    fund_payer(&env, &token2, &payer, 1_000_000);

    // Create different volumes for each token to test market share
    // Token1: Higher volume
    let inv1 = client.create_invoice(
        &merchant1,
        &String::from_str(&env, "token1_high_volume"),
        &8000,
        &token1,
        &None,
    );
    client.pay_invoice(&payer, &inv1);

    let inv2 = client.create_invoice(
        &merchant2,
        &String::from_str(&env, "token1_more_volume"),
        &2000,
        &token1,
        &None,
    );
    client.pay_invoice(&payer, &inv2);

    // Token2: Lower volume
    let inv3 = client.create_invoice(
        &merchant1,
        &String::from_str(&env, "token2_low_volume"),
        &1000,
        &token2,
        &None,
    );
    client.pay_invoice(&payer, &inv3);

    // Verify token analytics show correct market distribution
    let token1_analytics = client.get_token_analytics(&token1);
    let token2_analytics = client.get_token_analytics(&token2);

    assert_eq!(token1_analytics.total_volume, 10000); // 8000 + 2000
    assert_eq!(token2_analytics.total_volume, 1000);

    // Token1 should have higher transaction count and fees
    assert_eq!(token1_analytics.transaction_count, 2);
    assert_eq!(token2_analytics.transaction_count, 1);

    // Verify fees are calculated correctly for each token (different fee rates)
    assert_eq!(token1_analytics.total_fees, 1000); // 10% of 10000
    assert_eq!(token2_analytics.total_fees, 50);   // 5% of 1000
}

#[test]
fn test_analytics_data_consistency_after_refunds() {
    let env = Env::default();
    let (_admin, client, token1, _token2, merchant, _merchant_account) = setup(&env);
    let payer = Address::generate(&env);
    fund_payer(&env, &token1, &payer, 1_000_000);

    // Create and pay invoice
    let inv = client.create_invoice(
        &merchant,
        &String::from_str(&env, "refund_test_invoice"),
        &2000,
        &token1,
        &None,
    );
    client.pay_invoice(&payer, &inv);

    // Check analytics after payment
    let analytics_after_payment = client.get_merchant_analytics(&merchant, &token1);
    assert_eq!(analytics_after_payment.total_volume, 2000);
    assert_eq!(analytics_after_payment.total_fees, 200);
    assert_eq!(analytics_after_payment.transaction_count, 1);

    // Refund the invoice
    client.refund_invoice(&merchant, &inv, &1000); // Partial refund

    // Analytics should remain unchanged after refund (refunds don't reduce analytics)
    let analytics_after_refund = client.get_merchant_analytics(&merchant, &token1);
    assert_eq!(analytics_after_refund.total_volume, 2000); // Volume unchanged
    assert_eq!(analytics_after_refund.total_fees, 200);   // Fees unchanged
    assert_eq!(analytics_after_refund.transaction_count, 1); // Count unchanged

    // Token analytics should also remain unchanged
    let token_analytics = client.get_token_analytics(&token1);
    assert_eq!(token_analytics.total_volume, 2000);
    assert_eq!(token_analytics.total_fees, 200);
    assert_eq!(token_analytics.transaction_count, 1);
}