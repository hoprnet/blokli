//! Classification of the errors an RPC node returns when it refuses a raw transaction.
//!
//! Nodes report these as free text whose wording differs between clients (geth-style
//! "nonce too low" vs Nethermind-style "OldNonce"). Classifying them lets callers act on the
//! reason: whether the nonce was consumed, whether the transaction is already in the mempool,
//! or whether it must be re-signed with higher fees.

/// Why an RPC node refused to accept a raw transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BroadcastRejection {
    /// The node already holds this exact transaction.
    AlreadyKnown,
    /// Another transaction with the same nonce is pending and this one does not pay enough more to replace it.
    ReplacementUnderpriced,
    /// The nonce was already used by a mined transaction.
    NonceTooLow,
    /// The nonce is ahead of the signer's next nonce.
    NonceTooHigh,
    /// The fees are below the block base fee or the node's minimum.
    FeeTooLow,
    /// The signer cannot pay `gas_limit × max_fee_per_gas + value`.
    InsufficientFunds,
}

impl BroadcastRejection {
    /// Stable label for logs and metrics.
    pub fn label(self) -> &'static str {
        match self {
            BroadcastRejection::AlreadyKnown => "already_known",
            BroadcastRejection::ReplacementUnderpriced => "replacement_underpriced",
            BroadcastRejection::NonceTooLow => "nonce_too_low",
            BroadcastRejection::NonceTooHigh => "nonce_too_high",
            BroadcastRejection::FeeTooLow => "fee_too_low",
            BroadcastRejection::InsufficientFunds => "insufficient_funds",
        }
    }
}

/// Patterns matched against the error message, lowercased and stripped of spaces, `_` and `-`.
///
/// Order matters: replacement errors also contain "underpriced", so they are checked before the
/// generic fee patterns.
const PATTERNS: &[(&str, BroadcastRejection)] = &[
    ("alreadyknown", BroadcastRejection::AlreadyKnown),
    ("knowntransaction", BroadcastRejection::AlreadyKnown),
    ("alreadyimported", BroadcastRejection::AlreadyKnown),
    (
        "replacementtransactionunderpriced",
        BroadcastRejection::ReplacementUnderpriced,
    ),
    ("replacementunderpriced", BroadcastRejection::ReplacementUnderpriced),
    ("replacementnotallowed", BroadcastRejection::ReplacementUnderpriced),
    ("noncetoolow", BroadcastRejection::NonceTooLow),
    ("oldnonce", BroadcastRejection::NonceTooLow),
    ("noncetoohigh", BroadcastRejection::NonceTooHigh),
    ("noncegap", BroadcastRejection::NonceTooHigh),
    ("noncetoofarinfuture", BroadcastRejection::NonceTooHigh),
    ("insufficientfunds", BroadcastRejection::InsufficientFunds),
    ("lessthanblockbasefee", BroadcastRejection::FeeTooLow),
    ("feecaptoolow", BroadcastRejection::FeeTooLow),
    ("feetoolow", BroadcastRejection::FeeTooLow),
    ("gaspricetoolow", BroadcastRejection::FeeTooLow),
    ("underpriced", BroadcastRejection::FeeTooLow),
];

/// Classify an RPC node's refusal of a raw transaction from its error message.
///
/// Returns `None` for transport failures and for errors that do not match a known reason.
pub fn classify_broadcast_error(message: &str) -> Option<BroadcastRejection> {
    let normalized: String = message
        .chars()
        .filter(|c| !matches!(c, ' ' | '_' | '-'))
        .flat_map(char::to_lowercase)
        .collect();

    PATTERNS
        .iter()
        .find(|(pattern, _)| normalized.contains(pattern))
        .map(|(_, rejection)| *rejection)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_geth_style_messages() {
        let cases = [
            ("already known", BroadcastRejection::AlreadyKnown),
            ("known transaction: 0xabc", BroadcastRejection::AlreadyKnown),
            (
                "replacement transaction underpriced",
                BroadcastRejection::ReplacementUnderpriced,
            ),
            (
                "nonce too low: next nonce 5, tx nonce 4",
                BroadcastRejection::NonceTooLow,
            ),
            ("nonce too high", BroadcastRejection::NonceTooHigh),
            (
                "max fee per gas less than block base fee: address 0x1, maxFeePerGas: 21, baseFee: 426",
                BroadcastRejection::FeeTooLow,
            ),
            ("transaction underpriced", BroadcastRejection::FeeTooLow),
            (
                "insufficient funds for gas * price + value",
                BroadcastRejection::InsufficientFunds,
            ),
        ];
        for (message, expected) in cases {
            assert_eq!(classify_broadcast_error(message), Some(expected), "{message}");
        }
    }

    #[test]
    fn classifies_nethermind_style_messages() {
        let cases = [
            ("AlreadyKnown", BroadcastRejection::AlreadyKnown),
            (
                "OldNonce, Current nonce: 5, nonce of rejected tx: 4",
                BroadcastRejection::NonceTooLow,
            ),
            ("NonceGap, Future nonce", BroadcastRejection::NonceTooHigh),
            ("NonceTooFarInFuture", BroadcastRejection::NonceTooHigh),
            ("FeeTooLow, MaxFeePerGas too low", BroadcastRejection::FeeTooLow),
            ("FeeTooLowToCompete", BroadcastRejection::FeeTooLow),
            ("ReplacementNotAllowed", BroadcastRejection::ReplacementUnderpriced),
            (
                "InsufficientFunds, Account balance: 0",
                BroadcastRejection::InsufficientFunds,
            ),
        ];
        for (message, expected) in cases {
            assert_eq!(classify_broadcast_error(message), Some(expected), "{message}");
        }
    }

    #[test]
    fn classifies_messages_wrapped_by_the_transport() {
        assert_eq!(
            classify_broadcast_error("RPC error: server returned an error response: error code -32000: nonce too low"),
            Some(BroadcastRejection::NonceTooLow)
        );
    }

    #[test]
    fn leaves_unknown_and_transport_errors_unclassified() {
        assert_eq!(classify_broadcast_error("error sending request for url"), None);
        assert_eq!(classify_broadcast_error("intrinsic gas too low"), None);
        assert_eq!(classify_broadcast_error(""), None);
    }
}
