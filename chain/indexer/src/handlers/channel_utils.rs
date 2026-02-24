// Re-export channel decoding from the shared crate so that
// consumers within the indexer (e.g. channels.rs) can continue
// importing from this module without changes.
pub(super) use blokli_chain_types::channel::decode_channel;
