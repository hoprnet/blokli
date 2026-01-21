use blokli_chain_rpc::HoprIndexerRpcOperations;
use blokli_db::{BlokliDbAllOperations, safe_contracts::PRESEEDED_BLOCK};
use hopr_primitive_types::prelude::{Address, ToHex};
use tracing::{info, warn};

use crate::errors::{CoreEthereumIndexerError, Result};

#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct RefreshStats {
    pub updated: usize,
    pub unchanged: usize,
    pub errors: usize,
}

fn address_from_bytes(bytes: &[u8], context: &str) -> Result<Address> {
    Address::try_from(bytes)
        .map_err(|_| CoreEthereumIndexerError::ProcessError(format!("invalid address bytes for {context}")))
}

pub async fn refresh_preseeded_safe_modules<T, Db>(db: &Db, rpc: &T) -> Result<RefreshStats>
where
    T: HoprIndexerRpcOperations,
    Db: BlokliDbAllOperations,
{
    let preseeded_safes = db.get_preseeded_safes(None).await?;

    info!(count = preseeded_safes.len(), "Found pre-seeded safes to refresh");

    let mut stats = RefreshStats::default();
    let refresh_block = PRESEEDED_BLOCK as u64 + 1;

    for safe in preseeded_safes {
        let safe_address = address_from_bytes(&safe.address, "safe address")?;
        let current_module = address_from_bytes(&safe.module_address, "module address")?;

        match rpc.get_hopr_module_from_safe(safe_address).await {
            Ok(Some(rpc_module)) if rpc_module != current_module => {
                info!(
                    safe = %safe_address.to_hex(),
                    old_module = %current_module.to_hex(),
                    new_module = %rpc_module.to_hex(),
                    "Updating pre-seeded safe module"
                );

                db.update_safe_module_address(None, safe_address, rpc_module, refresh_block, 0, 0)
                    .await?;
                stats.updated += 1;
            }
            Ok(_) => {
                stats.unchanged += 1;
            }
            Err(e) => {
                warn!(safe = %safe_address.to_hex(), error = %e, "RPC fetch failed");
                stats.errors += 1;
            }
        }
    }

    Ok(stats)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use async_trait::async_trait;
    use blokli_chain_rpc::{BlockWithLogs, FilterSet, errors::RpcError};
    use blokli_db::{db::BlokliDb, safe_contracts::BlokliDbSafeContractOperations};
    use futures::Stream;
    use hopr_crypto_types::types::Hash;
    use hopr_primitive_types::prelude::{Address, HoprBalance, XDaiBalance};

    use super::*;

    fn random_address() -> Address {
        Address::from(hopr_crypto_random::random_bytes())
    }

    struct TestRpc {
        modules: HashMap<Address, Option<Address>>,
    }

    #[async_trait]
    impl HoprIndexerRpcOperations for TestRpc {
        async fn block_number(&self) -> blokli_chain_rpc::errors::Result<u64> {
            Err(RpcError::Other("unused".into()))
        }

        async fn get_transaction_sender(&self, _tx_hash: Hash) -> blokli_chain_rpc::errors::Result<Address> {
            Err(RpcError::Other("unused".into()))
        }

        fn try_stream_logs<'a>(
            &'a self,
            _start_block_number: u64,
            _filters: FilterSet,
            _is_synced: bool,
        ) -> blokli_chain_rpc::errors::Result<std::pin::Pin<Box<dyn Stream<Item = BlockWithLogs> + Send + 'a>>>
        {
            Err(RpcError::Other("unused".into()))
        }

        async fn get_xdai_balance(&self, _address: Address) -> blokli_chain_rpc::errors::Result<XDaiBalance> {
            Err(RpcError::Other("unused".into()))
        }

        async fn get_hopr_balance(&self, _address: Address) -> blokli_chain_rpc::errors::Result<HoprBalance> {
            Err(RpcError::Other("unused".into()))
        }

        async fn get_hopr_allowance(
            &self,
            _owner: Address,
            _spender: Address,
        ) -> blokli_chain_rpc::errors::Result<HoprBalance> {
            Err(RpcError::Other("unused".into()))
        }

        async fn get_transaction_count(&self, _address: Address) -> blokli_chain_rpc::errors::Result<u64> {
            Err(RpcError::Other("unused".into()))
        }

        async fn get_channel_closure_notice_period(&self) -> blokli_chain_rpc::errors::Result<std::time::Duration> {
            Err(RpcError::Other("unused".into()))
        }

        async fn get_hopr_module_from_safe(
            &self,
            safe_address: Address,
        ) -> blokli_chain_rpc::errors::Result<Option<Address>> {
            Ok(self.modules.get(&safe_address).cloned().unwrap_or(None))
        }
    }

    #[tokio::test]
    async fn test_refresh_preseeded_safe_modules() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let safe_address = random_address();
        let module_old = random_address();
        let module_new = random_address();
        let chain_key = random_address();

        db.upsert_safe_contract(None, safe_address, module_old, chain_key, PRESEEDED_BLOCK as u64, 0, 0)
            .await?;

        let mut modules = HashMap::new();
        modules.insert(safe_address, Some(module_new));
        let rpc = TestRpc { modules };

        let stats = refresh_preseeded_safe_modules(&db, &rpc).await?;
        assert_eq!(stats.updated, 1);
        assert_eq!(stats.unchanged, 0);
        assert_eq!(stats.errors, 0);

        let entry = db
            .get_safe_contract_by_address(None, safe_address)
            .await?
            .expect("safe should exist");
        assert_eq!(entry.module_address, module_new.as_ref().to_vec());

        Ok(())
    }
}
