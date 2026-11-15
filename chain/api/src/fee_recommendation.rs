//! Recommended EIP-1559 fees, shared by the `chainInfo` query and the transaction monitor.

use blokli_chain_rpc::HoprRpcOperations;

/// Lower bounds, in wei, for the EIP-1559 fee estimates
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GasFeeFloors {
    /// Minimum `maxPriorityFeePerGas`
    pub min_priority_fee_per_gas: u128,
    /// Minimum `maxFeePerGas`
    pub min_max_fee_per_gas: u128,
}

/// EIP-1559 fees, in wei, a transaction should currently be signed with
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecommendedFees {
    /// Recommended `maxFeePerGas`
    pub max_fee_per_gas: u128,
    /// Recommended `maxPriorityFeePerGas`
    pub max_priority_fee_per_gas: u128,
}

/// Fetch the node's EIP-1559 estimate, scale it by `gas_multiplier` and raise it to `floors`.
///
/// # Errors
/// Returns the RPC error when the estimate cannot be fetched.
pub async fn recommended_fees<R: HoprRpcOperations + ?Sized>(
    rpc: &R,
    gas_multiplier: f64,
    floors: GasFeeFloors,
) -> blokli_chain_rpc::errors::Result<RecommendedFees> {
    let estimate = rpc.estimate_eip1559_fees().await?;
    let (max_fee_per_gas, max_priority_fee_per_gas) = apply_gas_fee_floors(
        scale_wei_by_multiplier(estimate.max_fee_per_gas, gas_multiplier),
        scale_wei_by_multiplier(estimate.max_priority_fee_per_gas, gas_multiplier),
        floors,
    );
    Ok(RecommendedFees {
        max_fee_per_gas,
        max_priority_fee_per_gas,
    })
}

pub fn scale_wei_by_multiplier(value: u128, multiplier: f64) -> u128 {
    let mut multiplier_string = format!("{multiplier:.12}");
    while multiplier_string.ends_with('0') {
        multiplier_string.pop();
    }
    if multiplier_string.ends_with('.') {
        multiplier_string.pop();
    }

    let (whole_part, fractional_part) = match multiplier_string.split_once('.') {
        Some(parts) => parts,
        None => (multiplier_string.as_str(), ""),
    };

    let whole = match whole_part.parse::<u128>() {
        Ok(parsed) => parsed,
        Err(_) => return u128::MAX,
    };

    let fractional = if fractional_part.is_empty() {
        0
    } else {
        match fractional_part.parse::<u128>() {
            Ok(parsed) => parsed,
            Err(_) => return u128::MAX,
        }
    };

    let mut denominator = 1u128;
    for _ in 0..fractional_part.len() {
        denominator = denominator.saturating_mul(10);
    }

    let numerator = match whole
        .checked_mul(denominator)
        .and_then(|scaled_whole| scaled_whole.checked_add(fractional))
    {
        Some(parsed) => parsed,
        None => return u128::MAX,
    };

    value
        .saturating_mul(numerator)
        .saturating_add(denominator.saturating_sub(1))
        / denominator
}

/// Raise `(max_fee_per_gas, max_priority_fee_per_gas)` to the configured floors.
///
/// The estimate's headroom for base fee growth (`max_fee - priority_fee`) is kept on top of
/// the raised priority fee, so lifting the priority fee never eats into it.
pub fn apply_gas_fee_floors(
    max_fee_per_gas: u128,
    max_priority_fee_per_gas: u128,
    floors: GasFeeFloors,
) -> (u128, u128) {
    let base_fee_headroom = max_fee_per_gas.saturating_sub(max_priority_fee_per_gas);
    let priority_fee = max_priority_fee_per_gas.max(floors.min_priority_fee_per_gas);
    let max_fee = priority_fee
        .saturating_add(base_fee_headroom)
        .max(floors.min_max_fee_per_gas);
    (max_fee, priority_fee)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scale_wei_by_multiplier_identity() {
        assert_eq!(scale_wei_by_multiplier(1_000_000_000, 1.0), 1_000_000_000);
    }

    #[test]
    fn test_scale_wei_by_multiplier_rounds_up() {
        assert_eq!(scale_wei_by_multiplier(3, 1.5), 5);
    }

    const FLOORS: GasFeeFloors = GasFeeFloors {
        min_priority_fee_per_gas: 100,
        min_max_fee_per_gas: 5_000_000,
    };

    #[test]
    fn test_gas_fee_floors_lift_floor_level_estimate() {
        // Typical Gnosis estimate: 1 wei tip, 2 × 10 wei base fee headroom
        assert_eq!(apply_gas_fee_floors(21, 1, FLOORS), (5_000_000, 100));
    }

    #[test]
    fn test_gas_fee_floors_keep_base_fee_headroom() {
        // Base fee spike: 2 × 450_000 wei headroom must survive the priority fee lift
        assert_eq!(apply_gas_fee_floors(900_001, 1, FLOORS), (5_000_000, 100));
        assert_eq!(apply_gas_fee_floors(80_000_001, 1, FLOORS), (80_000_100, 100));
    }

    #[test]
    fn test_gas_fee_floors_leave_higher_estimates_unchanged() {
        assert_eq!(
            apply_gas_fee_floors(3_000_000_000, 1_000_000_000, FLOORS),
            (3_000_000_000, 1_000_000_000)
        );
    }

    #[test]
    fn test_gas_fee_floors_default_is_a_no_op() {
        assert_eq!(apply_gas_fee_floors(21, 1, GasFeeFloors::default()), (21, 1));
    }
}
