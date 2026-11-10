//! Blockchain positions used to order state changes
//!
//! A position is the `(block, tx_index, log_index)` tuple of the log that produced a state change.
//! Positions are ordered lexicographically.

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_block_position_ordering() {
        let pos1 = BlockPosition {
            block: 100,
            tx_index: 5,
            log_index: 2,
        };
        let pos2 = BlockPosition {
            block: 100,
            tx_index: 5,
            log_index: 3,
        };
        let pos3 = BlockPosition {
            block: 100,
            tx_index: 6,
            log_index: 1,
        };
        let pos4 = BlockPosition {
            block: 101,
            tx_index: 0,
            log_index: 0,
        };

        assert!(pos1 < pos2);
        assert!(pos2 < pos3);
        assert!(pos3 < pos4);
    }
}
