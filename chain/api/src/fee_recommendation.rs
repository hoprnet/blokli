//! Recommended EIP-1559 fees, shared by the `chainInfo` query and the transaction monitor.

use blokli_chain_rpc::{FeeHistorySnapshot, HoprRpcOperations};

/// Policy, in wei, applied to the EIP-1559 fee estimates exposed by chainInfo and used by the transaction monitor
///
/// It holds floors, a tip cap, a max fee ceiling and a base fee headroom factor.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GasFeePolicy {
    /// Minimum `maxPriorityFeePerGas`
    pub min_priority_fee_per_gas: u128,
    /// Maximum `maxPriorityFeePerGas`, so the tip paid per transaction stays bounded
    pub max_priority_fee_per_gas: u128,
    /// Minimum `maxFeePerGas`
    pub min_max_fee_per_gas: u128,
    /// Maximum `maxFeePerGas`, which bounds the balance reserved upfront (gas limit x max fee)
    pub max_max_fee_per_gas: u128,
    /// Factor applied to the next-block base fee to leave room for base fee growth
    pub base_fee_headroom_factor: f64,
}

impl Default for GasFeePolicy {
    /// No floors, no tip cap and no extra headroom
    fn default() -> Self {
        Self {
            min_priority_fee_per_gas: 0,
            max_priority_fee_per_gas: u128::MAX,
            min_max_fee_per_gas: 0,
            max_max_fee_per_gas: u128::MAX,
            base_fee_headroom_factor: 1.0,
        }
    }
}

/// EIP-1559 fees, in wei, a transaction should currently be signed with
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecommendedFees {
    /// Recommended `maxFeePerGas`
    pub max_fee_per_gas: u128,
    /// Recommended `maxPriorityFeePerGas`
    pub max_priority_fee_per_gas: u128,
}

/// Fetch the fee snapshot, scale it by `gas_multiplier` and apply `policy`.
///
/// Uses the `eth_feeHistory` snapshot and falls back to the node's own estimate when it fails, as `chainInfo` does.
///
/// # Errors
/// Returns an error when no snapshot can be fetched, or when the policy ceiling is below the next-block base fee,
/// since the capped max fee could not be included in the next block.
pub async fn recommended_fees<R: HoprRpcOperations + ?Sized>(
    rpc: &R,
    gas_multiplier: f64,
    policy: GasFeePolicy,
) -> Result<RecommendedFees, String> {
    let snapshot: FeeHistorySnapshot = match rpc.get_fee_history_snapshot().await {
        Ok(snapshot) => snapshot,
        Err(_) => rpc.get_node_fee_snapshot().await.map_err(|e| e.to_string())?,
    };
    let (max_fee_per_gas, max_priority_fee_per_gas) = fees_from_fee_history(snapshot, gas_multiplier, policy)
        .ok_or_else(|| "the configured max fee ceiling is below the next-block base fee".to_string())?;
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

/// Bound the max fee to the policy ceiling and make sure it never falls below the tip.
///
/// This is applied last, so the ceiling wins over the floor, the headroom and the multiplier.
fn bound_max_fee(max_fee_per_gas: u128, priority_fee_per_gas: u128, policy: GasFeePolicy) -> u128 {
    max_fee_per_gas
        .max(policy.min_max_fee_per_gas)
        .min(policy.max_max_fee_per_gas)
        .max(priority_fee_per_gas)
}

/// Compute `(max_fee_per_gas, max_priority_fee_per_gas)` from a raw fee snapshot.
///
/// The tip is the recent median tip scaled by `gas_multiplier`, clamped to the configured priority fee bounds.
/// The max fee is that tip plus the scaled next-block base fee times `base_fee_headroom_factor`, raised to
/// `min_max_fee_per_gas` and capped at `max_max_fee_per_gas`.
///
/// Returns `None` when the ceiling is below the scaled next-block base fee: the capped max fee could not be
/// included in the next block, so it must not be offered as an estimate.
pub fn fees_from_fee_history(
    snapshot: FeeHistorySnapshot,
    gas_multiplier: f64,
    policy: GasFeePolicy,
) -> Option<(u128, u128)> {
    let base_fee = scale_wei_by_multiplier(snapshot.next_base_fee_per_gas, gas_multiplier);
    if policy.max_max_fee_per_gas < base_fee {
        return None;
    }
    let priority_fee = scale_wei_by_multiplier(snapshot.median_priority_fee_per_gas, gas_multiplier)
        .max(policy.min_priority_fee_per_gas)
        .min(policy.max_priority_fee_per_gas);
    let max_fee = bound_max_fee(
        priority_fee.saturating_add(scale_wei_by_multiplier(base_fee, policy.base_fee_headroom_factor)),
        priority_fee,
        policy,
    );
    Some((max_fee, priority_fee))
}
