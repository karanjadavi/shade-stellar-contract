# Analytics Aggregation Component Tests

This document describes the comprehensive test suite for the Analytics Aggregation Component in the Shade Stellar Contract.

## Overview

The analytics aggregation component tracks and aggregates payment data across multiple dimensions:
- **Merchant Analytics**: Volume, fees, and transaction counts per merchant per token
- **Token Analytics**: Global volume, fees, and transaction counts per token
- **Time-based Tracking**: Accurate timestamp recording for daily/weekly analysis
- **Cross-dimensional Aggregation**: Summary analytics across all tokens for merchants

## Test Coverage

### 1. Volume Increment Tests (`test_volume_increments_*`)

**Purpose**: Ensure volume calculations are accurate across different scenarios.

#### `test_volume_increments_single_merchant_single_token`
- Tests basic volume accumulation for a single merchant using one token
- Verifies transaction count increments correctly
- Validates fee calculations at each step
- **Acceptance Criteria**: ✅ Volume increments accurately with each payment

#### `test_volume_increments_single_merchant_multiple_tokens`
- Tests volume tracking when a merchant accepts multiple tokens
- Verifies token-specific analytics are maintained separately
- Validates merchant summary aggregates across all tokens
- **Acceptance Criteria**: ✅ Token tracking works correctly

#### `test_volume_increments_multiple_merchants`
- Tests volume tracking across different merchants
- Ensures merchant analytics are isolated from each other
- Verifies global token analytics aggregate across all merchants
- **Acceptance Criteria**: ✅ Volume increments work for multiple merchants

### 2. Time Bucketing Accuracy Tests (`test_time_bucketing_accuracy_*`)

**Purpose**: Verify timestamp accuracy for daily/weekly aggregations.

#### `test_time_bucketing_accuracy_daily`
- Tests analytics timestamp recording across daily boundaries
- Verifies `last_updated` field accuracy
- Ensures cumulative data spans multiple days correctly
- **Acceptance Criteria**: ✅ Daily time bucketing is accurate

#### `test_time_bucketing_accuracy_weekly`
- Tests analytics timestamp recording across weekly boundaries
- Verifies weekly aggregation periods
- Ensures data integrity across week transitions
- **Acceptance Criteria**: ✅ Weekly time bucketing is accurate

### 3. Token Tracking Tests (`test_token_analytics_*`)

**Purpose**: Ensure global token analytics work correctly.

#### `test_token_analytics_aggregation`
- Tests global token analytics across multiple merchants
- Verifies unique merchant counting
- Validates token-specific fee and volume aggregation
- **Acceptance Criteria**: ✅ Token tracking aggregates correctly

#### `test_analytics_token_market_dominance`
- Tests market share calculations between tokens
- Verifies different fee rates are applied correctly
- Validates transaction distribution across tokens
- **Acceptance Criteria**: ✅ Token market analysis works

### 4. Analytics Consistency Tests

#### `test_analytics_consistency_across_payment_types`
- Tests analytics with various payment amounts
- Verifies cumulative calculations remain accurate
- Validates fee calculations with different amounts
- **Acceptance Criteria**: ✅ All analytics tests pass

#### `test_analytics_data_integrity`
- Cross-validates merchant, token, and summary analytics
- Ensures data consistency across all analytics dimensions
- Verifies mathematical relationships between different analytics views
- **Acceptance Criteria**: ✅ Data integrity maintained

### 5. Advanced Scenarios

#### `test_analytics_with_volume_discounts`
- Tests analytics accuracy when volume discounts apply
- Verifies fee calculations change with discount tiers
- Ensures volume tracking triggers discounts correctly
- **Acceptance Criteria**: ✅ Volume discount integration works

#### `test_analytics_with_subscription_payments`
- Tests analytics with subscription-based payments
- Verifies recurring payment analytics
- Ensures subscription charges are tracked like regular payments
- **Acceptance Criteria**: ✅ Subscription analytics work

#### `test_analytics_performance`
- Tests aggregation performance with many transactions
- Verifies analytics remain accurate with high transaction volume
- Validates system performance under load
- **Acceptance Criteria**: ✅ Performance is acceptable

### 6. Edge Cases

#### `test_zero_amount_payments_analytics`
- Tests analytics with zero-amount transactions
- Verifies transaction counting vs. volume tracking
- Ensures edge cases don't break analytics
- **Acceptance Criteria**: ✅ Edge cases handled correctly

#### `test_analytics_cross_time_boundaries`
- Tests analytics across day/week boundaries
- Verifies timestamp accuracy at boundary conditions
- Ensures no data loss at time transitions
- **Acceptance Criteria**: ✅ Time boundary handling works

#### `test_analytics_data_consistency_after_refunds`
- Tests that refunds don't affect historical analytics
- Verifies analytics immutability for completed transactions
- Ensures refund operations don't corrupt analytics data
- **Acceptance Criteria**: ✅ Refunds don't affect analytics

## Data Structures Tested

### MerchantAnalytics
```rust
pub struct MerchantAnalytics {
    pub merchant: Address,
    pub token: Address,
    pub total_volume: i128,
    pub total_fees: i128,
    pub transaction_count: u64,
    pub last_updated: u64,
}
```

### TokenAnalytics
```rust
pub struct TokenAnalytics {
    pub token: Address,
    pub total_volume: i128,
    pub total_fees: i128,
    pub transaction_count: u64,
    pub unique_merchants: u64,
    pub last_updated: u64,
}
```

### MerchantAnalyticsSummary
```rust
pub struct MerchantAnalyticsSummary {
    pub merchant: Address,
    pub total_volume: i128,
    pub total_fees: i128,
    pub transaction_count: u64,
    pub last_updated: u64,
}
```

## Running the Tests

To run the analytics aggregation tests:

```bash
# Run all analytics tests
cargo test test_analytics_aggregation

# Run specific test
cargo test test_volume_increments_single_merchant_single_token

# Run with output
cargo test test_analytics_aggregation -- --nocapture
```

## Test Environment Setup

Each test uses a standardized setup:
1. Mock Stellar environment
2. Contract deployment and initialization
3. Token creation and acceptance
4. Merchant registration and verification
5. Fee configuration (10% for token1, 5% for token2)

## Validation Criteria

All tests validate:
- ✅ **Volume Accuracy**: Cumulative volume calculations are correct
- ✅ **Fee Calculations**: Fees are calculated and tracked accurately
- ✅ **Transaction Counting**: Transaction counts increment properly
- ✅ **Timestamp Accuracy**: Time tracking works for daily/weekly analysis
- ✅ **Data Consistency**: All analytics views remain mathematically consistent
- ✅ **Token Isolation**: Different tokens maintain separate analytics
- ✅ **Merchant Isolation**: Different merchants maintain separate analytics
- ✅ **Cross-dimensional Aggregation**: Summary views aggregate correctly

## Integration Points

The analytics system integrates with:
- **Payment Processing**: Records analytics on successful payments
- **Subscription Charging**: Records analytics on subscription charges
- **Fee Calculation**: Uses volume data for discount calculations
- **Volume Discounts**: Triggers based on accumulated volume
- **Admin Functions**: Provides analytics retrieval endpoints

## Performance Considerations

The tests verify:
- Analytics updates are atomic with payment processing
- Large numbers of transactions don't degrade performance
- Storage efficiency for high-volume merchants
- Query performance for analytics retrieval