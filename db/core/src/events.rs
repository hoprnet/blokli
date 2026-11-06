//! Block position type used for ordering events and temporal queries

/// Position in the blockchain for ordering events and temporal queries
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct BlockPosition {
    pub block: i64,
    pub tx_index: i64,
    pub log_index: i64,
}

impl BlockPosition {
    /// Create a new BlockPosition
    pub fn new(block: i64, tx_index: i64, log_index: i64) -> Self {
        Self {
            block,
            tx_index,
            log_index,
        }
    }

    /// Create a BlockPosition at the start of a block
    pub fn at_block(block: i64) -> Self {
        Self {
            block,
            tx_index: 0,
            log_index: 0,
        }
    }
}
