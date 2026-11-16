//! Macro implementation for historical bindings catalog entries.

#[cfg(test)]
pub(crate) mod test_support {
    pub(crate) use hopr_bindings::{
        exports::alloy::{
            primitives::{Address as AlloyAddress, B256, Log as AlloyLog},
            sol_types::{SolEvent, SolEventInterface},
        },
        hopr_node_stake_factory::HoprNodeStakeFactory::{
            HoprNodeStakeFactoryEvents as CurrentStakeFactoryEvents,
            NewHoprNodeStakeModuleForSafe as CurrentStakeFactoryEvent,
        },
    };
}

macro_rules! historical_bindings {
    () => {
        /// Resolves a StakeFactory from the deployment manifest of a supported historical release.
        pub fn resolve_stake_factory(
            _release: &ContractsRelease,
            _network_name: &str,
        ) -> Option<StakeFactoryDeployment> {
            None
        }
    };
    ($($release:literal => $bindings:ident),+ $(,)?) => {
        #[cfg(test)]
        use $crate::macros::test_support::{
            AlloyAddress,
            AlloyLog,
            B256,
            CurrentStakeFactoryEvent,
            CurrentStakeFactoryEvents,
            SolEvent,
            SolEventInterface,
        };

        /// Resolves a StakeFactory from the deployment manifest of a supported historical release.
        pub fn resolve_stake_factory(
            release: &ContractsRelease,
            network_name: &str,
        ) -> Option<StakeFactoryDeployment> {
            match release.as_str() {
                $(
                    $release => {
                        let networks = $bindings::config::NetworksWithContractAddresses::default();
                        let network = networks.networks.get(network_name)?;
                        let address_bytes: [u8; 20] = network
                            .addresses
                            .node_stake_factory
                            .as_slice()
                            .try_into()
                            .ok()?;

                        Some(StakeFactoryDeployment {
                            address: Address::from(address_bytes),
                            chain_id: network.chain_id,
                            indexer_start_block_number: network.indexer_start_block_number,
                        })
                    }
                )*
                _ => None,
            }
        }

        #[cfg(test)]
        #[test]
        fn test_historical_stake_factory_abi_compatibility() {
            $(
                {
                    type HistoricalEvent = $bindings::hopr_node_stake_factory::HoprNodeStakeFactory::NewHoprNodeStakeModuleForSafe;
                    type CurrentEvent = CurrentStakeFactoryEvent;

                    let historical_signature =
                        <HistoricalEvent as $bindings::exports::alloy::sol_types::SolEvent>::SIGNATURE_HASH;
                    let current_signature =
                        <CurrentEvent as SolEvent>::SIGNATURE_HASH;
                    assert_eq!(
                        historical_signature.as_slice(),
                        current_signature.as_slice(),
                        "StakeFactory event signature changed in contracts release {}",
                        $release,
                    );

                    let module_bytes = [0x11; 20];
                    let safe_bytes = [0x22; 20];
                    let historical_event = HistoricalEvent {
                        module: $bindings::exports::alloy::primitives::Address::from(module_bytes),
                        safe: $bindings::exports::alloy::primitives::Address::from(safe_bytes),
                    };
                    let historical_log =
                        <HistoricalEvent as $bindings::exports::alloy::sol_types::SolEvent>::encode_log_data(
                            &historical_event,
                        );
                    let current_topics = historical_log
                        .topics()
                        .iter()
                        .map(|topic| {
                            B256::from_slice(topic.as_slice())
                        })
                        .collect();
                    let current_log = AlloyLog::new(
                        AlloyAddress::ZERO,
                        current_topics,
                        historical_log.data.to_vec().into(),
                    )
                    .expect("historical StakeFactory event should produce valid log data");
                    let decoded = <CurrentStakeFactoryEvents as SolEventInterface>::decode_log(&current_log)
                    .unwrap_or_else(|error| {
                        panic!(
                            "StakeFactory event from contracts release {} is not decodable by current bindings: {error}",
                            $release,
                        )
                    });

                    match decoded.data {
                        CurrentStakeFactoryEvents::NewHoprNodeStakeModuleForSafe(event) => {
                            assert_eq!(event.module.as_slice(), module_bytes);
                            assert_eq!(event.safe.as_slice(), safe_bytes);
                        }
                        event => panic!(
                            "StakeFactory event from contracts release {} decoded as unexpected variant: {event:?}",
                            $release,
                        ),
                    }
                }
            )*
        }

        #[cfg(test)]
        #[test]
        fn test_historical_stake_factory_deployments() {
            $(
                {
                    let release = $release
                        .parse::<ContractsRelease>()
                        .expect("catalog release should be valid semver");
                    let networks = $bindings::config::NetworksWithContractAddresses::default();
                    assert!(
                        !networks.networks.is_empty(),
                        "contracts release {} should contain at least one network",
                        $release,
                    );

                    for (network_name, expected) in networks.networks {
                        let deployment = resolve_stake_factory(&release, &network_name).unwrap_or_else(|| {
                            panic!(
                                "contracts release {} should resolve network {}",
                                $release, network_name,
                            )
                        });

                        assert_eq!(deployment.chain_id, expected.chain_id);
                        assert_eq!(
                            deployment.indexer_start_block_number,
                            expected.indexer_start_block_number,
                        );
                        assert_eq!(
                            deployment.address.as_ref(),
                            expected.addresses.node_stake_factory.as_slice(),
                        );
                    }
                }
            )*
        }
    };
}

pub(crate) use historical_bindings;
