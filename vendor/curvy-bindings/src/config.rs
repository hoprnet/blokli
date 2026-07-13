use alloy::{
    contract::{RawCallBuilder, Result as ContractResult},
    network::TransactionBuilder,
    primitives::{Address, Bytes, U256},
    rpc::types::TransactionRequest,
    sol_types::{SolCall, SolEvent, SolValue},
};
use serde::{Deserialize, Serialize};
use serde_with::{DisplayFromStr, serde_as};
use tracing::debug;

use crate::{
    constants::*,
    curvy_aggregation_verifier::CurvyAggregationVerifier::{self, CurvyAggregationVerifierInstance},
    curvy_aggregator_alpha_v2::{
        CurvyAggregatorAlphaV2::{self, CurvyAggregatorAlphaV2Instance},
        CurvyTypes as AggregatorTypes,
    },
    curvy_pending_notes_commitment_verifier::CurvyPendingNotesCommitmentVerifier::{
        self, CurvyPendingNotesCommitmentVerifierInstance,
    },
    curvy_vault_v2::{
        CurvyTypes as VaultTypes,
        CurvyVaultV2::{self, CurvyVaultV2Instance},
    },
    curvy_withdrawal_verifier::CurvyWithdrawalVerifier::{self, CurvyWithdrawalVerifierInstance},
    erc1967_proxy::ERC1967Proxy,
    erc20_mock::ERC20Mock::{self, ERC20MockInstance},
    multicall3::Multicall3::{self, Multicall3Instance},
    portal_factory::PortalFactory::{self, PortalFactoryInstance},
    poseidon_t4::PoseidonT4::{self, PoseidonT4Instance},
};

/// The two CreateX entry points the deploy uses. Declared by hand (not bound from the
/// full ICreateX ABI) because that ABI overloads `ContractCreation`, which `sol!`
/// cannot disambiguate — the same proven subset the validated `curvy-deployer` used.
mod createx {
    alloy::sol! {
        #[allow(missing_docs)]
        event ContractCreation(address indexed newContract, bytes32 indexed salt);
        #[allow(missing_docs)]
        function deployCreate2(bytes32 salt, bytes initCode) external payable returns (address newContract);
    }
}

/// Holds addresses of all smart contracts.
#[serde_as]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CurvyContractAddresses {
    /// CreateX factory (canonical keyless-deployment address)
    #[serde_as(as = "DisplayFromStr")]
    pub createx: Address,
    /// PoseidonT4 library (linked into the aggregator implementation)
    #[serde_as(as = "DisplayFromStr")]
    pub poseidon_t4: Address,
    /// Aggregator implementation (UUPS)
    #[serde_as(as = "DisplayFromStr")]
    pub aggregator_impl: Address,
    /// Aggregator ERC1967 proxy — the address consumers talk to
    #[serde_as(as = "DisplayFromStr")]
    pub aggregator_proxy: Address,
    /// Groth16 aggregation verifier (2,3,30)
    #[serde_as(as = "DisplayFromStr")]
    pub aggregation_verifier: Address,
    /// Groth16 pending-notes-commitment verifier (5,30)
    #[serde_as(as = "DisplayFromStr")]
    pub pending_notes_commitment_verifier: Address,
    /// Groth16 withdrawal verifier (2,30)
    #[serde_as(as = "DisplayFromStr")]
    pub withdrawal_verifier: Address,
    /// Vault implementation (UUPS)
    #[serde_as(as = "DisplayFromStr")]
    pub vault_impl: Address,
    /// Vault ERC1967 proxy — the address consumers talk to
    #[serde_as(as = "DisplayFromStr")]
    pub vault_proxy: Address,
    /// PortalFactory (deterministic CreateX deployCreate2 deploy)
    #[serde_as(as = "DisplayFromStr")]
    pub portal_factory: Address,
    /// Multicall3 devenv utility
    #[serde_as(as = "DisplayFromStr")]
    pub multicall3: Address,
    /// Mock ERC20 devenv utility (`mockMint`)
    #[serde_as(as = "DisplayFromStr")]
    pub erc20_mock: Address,
}

impl IntoIterator for &CurvyContractAddresses {
    type IntoIter = std::vec::IntoIter<Address>;
    type Item = Address;

    fn into_iter(self) -> Self::IntoIter {
        vec![
            self.createx,
            self.poseidon_t4,
            self.aggregator_impl,
            self.aggregator_proxy,
            self.aggregation_verifier,
            self.pending_notes_commitment_verifier,
            self.withdrawal_verifier,
            self.vault_impl,
            self.vault_proxy,
            self.portal_factory,
            self.multicall3,
            self.erc20_mock,
        ]
        .into_iter()
    }
}

impl CurvyContractAddresses {
    /// The Ignition-style `deployed_addresses.json` shape (EIP-55 checksummed values),
    /// with the SAME keys the original Hardhat pipeline emitted (minus its skipped ENS
    /// keys, which no downstream consumer reads). `curvy-e2e` / `curvy-hopr-runner`
    /// read `CurvyAggregator#ERC1967Proxy`, `CurvyVault#ERC1967Proxy` and
    /// `PortalFactory#PortalFactory` — treat every key here as a stable contract.
    pub fn to_ignition_json(&self) -> serde_json::Value {
        let cs = |a: &Address| a.to_checksum(None);
        serde_json::json!({
            "PortalFactory#CreateX": cs(&self.createx),
            "CurvyAggregator#PoseidonT4": cs(&self.poseidon_t4),
            "CurvyAggregator#CurvyAggregatorV2Implementation": cs(&self.aggregator_impl),
            "CurvyAggregator#ERC1967Proxy": cs(&self.aggregator_proxy),
            "CurvyAggregator#CurvyAggregatorAlphaV2": cs(&self.aggregator_proxy),
            "CurvyAggregator#CurvyAggregationVerifier": cs(&self.aggregation_verifier),
            "CurvyAggregator#CurvyPendingNotesCommitmentVerifier": cs(&self.pending_notes_commitment_verifier),
            "CurvyAggregator#CurvyWithdrawalVerifier": cs(&self.withdrawal_verifier),
            "CurvyVault#CurvyVaultV2Implementation": cs(&self.vault_impl),
            "CurvyVault#ERC1967Proxy": cs(&self.vault_proxy),
            "CurvyVault#CurvyVaultV2": cs(&self.vault_proxy),
            "PortalFactory#PortalFactory": cs(&self.portal_factory),
            "Devenv#Multicall3": cs(&self.multicall3),
            "Devenv#ERC20Mock": cs(&self.erc20_mock),
        })
    }
}

/// Holds instances to contracts.
/// `aggregator` / `vault` point at the ERC1967 proxies (the addresses consumers talk
/// to); the implementation instances are included so the full address set can be
/// reconstructed, mirroring how hopr-bindings keeps `module_implementation` around.
#[derive(Debug, Clone)]
pub struct CurvyContractInstances<P> {
    pub aggregator: CurvyAggregatorAlphaV2Instance<P>,
    pub vault: CurvyVaultV2Instance<P>,
    pub portal_factory: PortalFactoryInstance<P>,
    pub aggregation_verifier: CurvyAggregationVerifierInstance<P>,
    pub pending_notes_commitment_verifier: CurvyPendingNotesCommitmentVerifierInstance<P>,
    pub withdrawal_verifier: CurvyWithdrawalVerifierInstance<P>,
    pub aggregator_implementation: CurvyAggregatorAlphaV2Instance<P>,
    pub vault_implementation: CurvyVaultV2Instance<P>,
    pub poseidon_t4: PoseidonT4Instance<P>,
    pub multicall3: Multicall3Instance<P>,
    pub erc20_mock: ERC20MockInstance<P>,
}

impl<P> CurvyContractInstances<P>
where
    P: alloy::providers::Provider + Clone,
{
    pub fn new(contract_addresses: &CurvyContractAddresses, provider: P) -> Self {
        Self {
            aggregator: CurvyAggregatorAlphaV2Instance::new(contract_addresses.aggregator_proxy, provider.clone()),
            vault: CurvyVaultV2Instance::new(contract_addresses.vault_proxy, provider.clone()),
            portal_factory: PortalFactoryInstance::new(contract_addresses.portal_factory, provider.clone()),
            aggregation_verifier: CurvyAggregationVerifierInstance::new(
                contract_addresses.aggregation_verifier,
                provider.clone(),
            ),
            pending_notes_commitment_verifier: CurvyPendingNotesCommitmentVerifierInstance::new(
                contract_addresses.pending_notes_commitment_verifier,
                provider.clone(),
            ),
            withdrawal_verifier: CurvyWithdrawalVerifierInstance::new(
                contract_addresses.withdrawal_verifier,
                provider.clone(),
            ),
            aggregator_implementation: CurvyAggregatorAlphaV2Instance::new(
                contract_addresses.aggregator_impl,
                provider.clone(),
            ),
            vault_implementation: CurvyVaultV2Instance::new(contract_addresses.vault_impl, provider.clone()),
            poseidon_t4: PoseidonT4Instance::new(contract_addresses.poseidon_t4, provider.clone()),
            multicall3: Multicall3Instance::new(contract_addresses.multicall3, provider.clone()),
            erc20_mock: ERC20MockInstance::new(contract_addresses.erc20_mock, provider.clone()),
        }
    }

    /// Ensure the CreateX factory is live at its canonical address; if absent, fund the
    /// keyless deployer and publish the pre-signed (Nick's-method) raw deploy tx.
    /// Idempotent: skipped when CreateX code is already present.
    pub async fn deploy_createx_factory(provider: P) -> ContractResult<()> {
        let code = provider.get_code_at(CREATEX_ADDRESS).await?;
        if code.is_empty() {
            debug!("deploying CreateX factory...");
            // Fund CreateX deployer and deploy CreateX
            let tx = TransactionRequest::default()
                .with_to(CREATEX_DEPLOYER)
                .with_value(ETH_VALUE_FOR_CREATEX_DEPLOYER);

            // Sequentially executing the following transactions:
            // 1. Fund the deployer wallet
            provider.send_transaction(tx.clone()).await?.watch().await?;
            // 2. Use the funded deployer wallet to deploy CreateX with a signed txn
            let raw = hex::decode(CREATEX_SIGNED_DEPLOYMENT_TX).expect("CreateX deploy tx is valid hex");
            provider.send_raw_transaction(&raw).await?.watch().await?;
        }
        let code = provider.get_code_at(CREATEX_ADDRESS).await?;
        assert!(!code.is_empty(), "CreateX factory not deployed at {CREATEX_ADDRESS}");
        Ok(())
    }

    /// Deploys testing environment via the given provider: the full Curvy v2 suite
    /// (CreateX bootstrap, PoseidonT4 link, UUPS impl+proxy pairs, verifier
    /// registration, bilateral wiring, dev funding) — the deploy/wire half of the
    /// validated `curvy-deployer` pipeline (mirrors `Devenv.ts` minus ENS).
    async fn inner_deploy_full_suite_for_testing(provider: P, deployer_address: Address) -> ContractResult<Self> {
        // Pre-deploy the CreateX factory (needed for the deterministic PortalFactory)
        CurvyContractInstances::deploy_createx_factory(provider.clone()).await?;

        debug!("deploying contracts...");

        // 1. Aggregator module: PoseidonT4 → implementation (linked) → proxy →
        //    3 verifiers → verifier registration
        let poseidon_t4 = PoseidonT4::deploy(provider.clone()).await?;

        // The aggregator implementation links the PoseidonT4 library: substitute the
        // solc link placeholder in the unlinked creation bytecode with the freshly
        // deployed library address (the same proven scheme the validated deployer
        // used; solc `linkReferences`: one 20-byte slot).
        let unlinked = CURVY_AGGREGATOR_ALPHA_V2_UNLINKED_BYTECODE.trim();
        assert_eq!(
            unlinked.matches(POSEIDON_T4_LINK_PLACEHOLDER).count(),
            1,
            "expected exactly one PoseidonT4 link placeholder in the aggregator bytecode"
        );
        let linked = unlinked.replace(
            POSEIDON_T4_LINK_PLACEHOLDER,
            &hex::encode(poseidon_t4.address().as_slice()),
        );
        assert!(!linked.contains("__$"), "unresolved link placeholder(s) remain");
        let linked_code = hex::decode(&linked).expect("linked aggregator bytecode is valid hex");
        let aggregator_implementation_address =
            RawCallBuilder::new_raw_deploy(provider.clone(), linked_code.into())
                .deploy()
                .await?;
        let aggregator_implementation =
            CurvyAggregatorAlphaV2Instance::new(aggregator_implementation_address, provider.clone());

        let aggregator_initialize = CurvyAggregatorAlphaV2::initializeCall {
            initialOwner: deployer_address,
        }
        .abi_encode();
        let aggregator_proxy = ERC1967Proxy::deploy(
            provider.clone(),
            aggregator_implementation_address,
            aggregator_initialize.into(),
        )
        .await?;
        let aggregator = CurvyAggregatorAlphaV2Instance::new(*aggregator_proxy.address(), provider.clone());

        let aggregation_verifier = CurvyAggregationVerifier::deploy(provider.clone()).await?;
        let pending_notes_commitment_verifier =
            CurvyPendingNotesCommitmentVerifier::deploy(provider.clone()).await?;
        let withdrawal_verifier = CurvyWithdrawalVerifier::deploy(provider.clone()).await?;

        // Register the verifiers under their circuit dimensions (matches
        // `building-blocks/CurvyAggregator.ts`)
        aggregator
            .setPendingNotesCommitmentVerifier(
                PENDING_NOTES_COMMITMENT_BATCH_SIZE,
                *pending_notes_commitment_verifier.address(),
            )
            .send()
            .await?
            .watch()
            .await?;
        aggregator
            .setAggregationVerifier(
                AGGREGATION_MAX_INPUTS,
                AGGREGATION_MAX_OUTPUTS,
                *aggregation_verifier.address(),
            )
            .send()
            .await?
            .watch()
            .await?;
        aggregator
            .setWithdrawalVerifier(WITHDRAWAL_MAX_INPUTS, *withdrawal_verifier.address())
            .send()
            .await?
            .watch()
            .await?;

        // 2. Vault module: implementation → proxy
        let vault_implementation = CurvyVaultV2::deploy(provider.clone()).await?;
        let vault_initialize = CurvyVaultV2::initializeCall {
            initialOwner: deployer_address,
        }
        .abi_encode();
        let vault_proxy = ERC1967Proxy::deploy(
            provider.clone(),
            *vault_implementation.address(),
            vault_initialize.into(),
        )
        .await?;
        let vault = CurvyVaultV2Instance::new(*vault_proxy.address(), provider.clone());

        // 3. PortalFactory via CreateX `deployCreate2(salt, bytecode ++ abi.encode(owner))`
        //    — deterministic address (salt + owner + init code)
        let mut portal_factory_init_code = PortalFactory::BYTECODE.to_vec();
        portal_factory_init_code.extend_from_slice(&deployer_address.abi_encode());
        let deploy_create2 = createx::deployCreate2Call {
            salt: LOCAL_CREATE2_SALT,
            initCode: Bytes::from(portal_factory_init_code),
        }
        .abi_encode();
        let receipt = provider
            .send_transaction(
                TransactionRequest::default()
                    .with_to(CREATEX_ADDRESS)
                    .with_input(Bytes::from(deploy_create2)),
            )
            .await?
            .get_receipt()
            .await?;
        assert!(receipt.status(), "createX.deployCreate2(PortalFactory) reverted");
        let portal_factory_address = receipt
            .logs()
            .iter()
            .find(|log| {
                log.inner.address == CREATEX_ADDRESS
                    && log.topic0() == Some(&createx::ContractCreation::SIGNATURE_HASH)
            })
            .and_then(|log| log.topics().get(1).copied())
            .map(Address::from_word)
            .expect("no ContractCreation(address,bytes32) log from CreateX in receipt");
        let portal_factory = PortalFactoryInstance::new(portal_factory_address, provider.clone());

        // 4. Devenv utilities
        let multicall3 = Multicall3::deploy(provider.clone()).await?;
        let erc20_mock = ERC20Mock::deploy(provider.clone()).await?;

        // 5. Bilateral wiring (matches `Devenv.ts` exactly)
        vault
            .setCurvyAggregatorAddress(*aggregator.address())
            .send()
            .await?
            .watch()
            .await?;
        aggregator
            .updateConfig(AggregatorTypes::AggregatorConfigurationUpdate {
                curvyVault: *vault.address(),
                portalFactory: portal_factory_address,
            })
            .send()
            .await?
            .watch()
            .await?;
        portal_factory
            .updateConfig(*vault.address(), *aggregator.address(), LOCAL_LIFI_DIAMOND)
            .send()
            .await?
            .watch()
            .await?;
        vault
            .registerToken(*erc20_mock.address())
            .send()
            .await?
            .watch()
            .await?;

        // 6. Dev-address funding (matches `Devenv.ts`): 1000 ETH + 1000 mock ERC20
        let tx = TransactionRequest::default()
            .with_to(DEV_SHIELDING_ADDRESS)
            .with_value(ETH_VALUE_FOR_DEV_SHIELDING_ADDRESS);
        provider.send_transaction(tx).await?.watch().await?;
        erc20_mock
            .mockMint(DEV_SHIELDING_ADDRESS, ERC20_VALUE_FOR_DEV_SHIELDING_ADDRESS)
            .send()
            .await?
            .watch()
            .await?;

        Ok(Self {
            aggregator,
            vault,
            portal_factory,
            aggregation_verifier,
            pending_notes_commitment_verifier,
            withdrawal_verifier,
            aggregator_implementation,
            vault_implementation,
            poseidon_t4,
            multicall3,
            erc20_mock,
        })
    }

    /// Deploys testing environment via the given provider: the full suite deploy plus
    /// the two MANDATORY post-deploy calls (`setPerTokenGasFees` with the precomputed
    /// commitment-gas-fee root, `setFeeNotePublicKey` with the DEV_FEE_COLLECTOR key —
    /// aggregation/withdrawal proofs revert without them), read-back verified. The
    /// direct analogue of how hopr-bindings' `deploy_for_testing` mints tokens and
    /// configures its oracles.
    pub async fn deploy_for_testing(provider: P, deployer_address: Address) -> ContractResult<Self> {
        let instances = Self::inner_deploy_full_suite_for_testing(provider.clone(), deployer_address).await?;

        debug!("initialising gas fees and fee-note key...");
        let gas_fees: Vec<_> = DEV_GAS_FEE_TOKEN_IDS
            .iter()
            .zip(DEV_PENDING_NOTE_COMMITMENT_FEES.iter())
            .map(|(token_id, commitment_fee)| VaultTypes::GasFees {
                tokenId: *token_id,
                portalDeployment: DEV_PORTAL_DEPLOYMENT_FEE,
                pendingNoteCommitment: *commitment_fee,
                withdrawal: DEV_WITHDRAWAL_FEE,
            })
            .collect();
        instances
            .vault
            .setPerTokenGasFees(gas_fees.clone(), DEV_COMMITMENT_GAS_FEE_ROOT)
            .send()
            .await?
            .watch()
            .await?;
        instances
            .aggregator
            .setFeeNotePublicKey(DEV_FEE_COLLECTOR_PUBLIC_KEY_X, DEV_FEE_COLLECTOR_PUBLIC_KEY_Y)
            .send()
            .await?
            .watch()
            .await?;

        // Read-back verification (ported from the validated deployer): the values the
        // circuits bind must round-trip exactly.
        let commitment_fee_root = instances.aggregator.commitmentFeeRoot().call().await?;
        assert_eq!(
            commitment_fee_root, DEV_COMMITMENT_GAS_FEE_ROOT,
            "commitmentFeeRoot read-back mismatch"
        );
        let fee_key_x = instances.aggregator.feeNotePublicKey(U256::ZERO).call().await?;
        let fee_key_y = instances.aggregator.feeNotePublicKey(U256::ONE).call().await?;
        assert_eq!(
            (fee_key_x, fee_key_y),
            (DEV_FEE_COLLECTOR_PUBLIC_KEY_X, DEV_FEE_COLLECTOR_PUBLIC_KEY_Y),
            "feeNotePublicKey read-back mismatch"
        );
        for wanted in &gas_fees {
            let got = instances.vault.perTokenGasFees(wanted.tokenId).call().await?;
            assert_eq!(
                (got.portalDeployment, got.pendingNoteCommitment, got.withdrawal),
                (wanted.portalDeployment, wanted.pendingNoteCommitment, wanted.withdrawal),
                "perTokenGasFees({}) read-back mismatch",
                wanted.tokenId
            );
        }

        Ok(Self { ..instances })
    }

    pub fn get_contract_addresses(&self) -> CurvyContractAddresses {
        CurvyContractAddresses {
            createx: CREATEX_ADDRESS, /* CreateX lives at its canonical keyless-deployment
                                       * address on every chain, so the constant is the
                                       * address */
            poseidon_t4: *self.poseidon_t4.address(),
            aggregator_impl: *self.aggregator_implementation.address(),
            aggregator_proxy: *self.aggregator.address(),
            aggregation_verifier: *self.aggregation_verifier.address(),
            pending_notes_commitment_verifier: *self.pending_notes_commitment_verifier.address(),
            withdrawal_verifier: *self.withdrawal_verifier.address(),
            vault_impl: *self.vault_implementation.address(),
            vault_proxy: *self.vault.address(),
            portal_factory: *self.portal_factory.address(),
            multicall3: *self.multicall3.address(),
            erc20_mock: *self.erc20_mock.address(),
        }
    }
}

impl<P> From<&CurvyContractInstances<P>> for CurvyContractAddresses
where
    P: alloy::providers::Provider + Clone,
{
    fn from(instances: &CurvyContractInstances<P>) -> Self {
        instances.get_contract_addresses()
    }
}

#[cfg(test)]
mod tests {
    use alloy::{node_bindings::Anvil, primitives::address, providers::ProviderBuilder};

    use super::*;

    #[tokio::test]
    async fn deploy_for_testing_deploys_wires_and_initialises() {
        let anvil = Anvil::new().spawn();
        let signer: alloy::signers::local::PrivateKeySigner = anvil.keys()[0].clone().into();
        let deployer_address = signer.address();
        let provider = ProviderBuilder::new().wallet(signer).connect_http(anvil.endpoint_url());

        // deploy_for_testing already asserts the read-back of the commitment-fee root,
        // the fee-note key and the gas-fee table internally.
        let instances = CurvyContractInstances::deploy_for_testing(provider, deployer_address)
            .await
            .expect("deploy_for_testing should succeed");

        let addresses = instances.get_contract_addresses();

        // CreateX determinism cross-check: with the anvil account-0 owner and the
        // devenv salt, PortalFactory must always land at the same CREATE2 address.
        // NOTE: this address differs from the Hardhat-pipeline one
        // (0x3c0C573B618D88F1a370bf18000f437c450D8125) because CREATE2 hashes the FULL
        // init code including the CBOR metadata blobs, and solc's ipfs metadata hash
        // legitimately differs between the Hardhat and Foundry builds of the same
        // source (see the parity gate in ../generate.sh — the executable bytecode is
        // byte-identical). No consumer hardcodes the factory address; it flows through
        // the Ignition JSON.
        assert_eq!(
            addresses.portal_factory,
            address!("410607362be76701CcE07841281e7352E63f2072")
        );

        // The Ignition-JSON downstream contract: exact key set, checksummed values,
        // proxy aliases pointing at the proxy address.
        let json = addresses.to_ignition_json();
        let expected_keys = [
            "PortalFactory#CreateX",
            "CurvyAggregator#PoseidonT4",
            "CurvyAggregator#CurvyAggregatorV2Implementation",
            "CurvyAggregator#ERC1967Proxy",
            "CurvyAggregator#CurvyAggregatorAlphaV2",
            "CurvyAggregator#CurvyAggregationVerifier",
            "CurvyAggregator#CurvyPendingNotesCommitmentVerifier",
            "CurvyAggregator#CurvyWithdrawalVerifier",
            "CurvyVault#CurvyVaultV2Implementation",
            "CurvyVault#ERC1967Proxy",
            "CurvyVault#CurvyVaultV2",
            "PortalFactory#PortalFactory",
            "Devenv#Multicall3",
            "Devenv#ERC20Mock",
        ];
        let object = json.as_object().expect("ignition json is an object");
        assert_eq!(object.len(), expected_keys.len());
        for key in expected_keys {
            assert!(object.contains_key(key), "missing ignition key {key}");
        }
        assert_eq!(json["CurvyAggregator#ERC1967Proxy"], json["CurvyAggregator#CurvyAggregatorAlphaV2"]);
        assert_eq!(json["CurvyVault#ERC1967Proxy"], json["CurvyVault#CurvyVaultV2"]);

        // `new` from addresses reconstructs the same instance set.
        let rebuilt = CurvyContractInstances::new(&addresses, instances.aggregator.provider().clone());
        assert_eq!(rebuilt.get_contract_addresses(), addresses);
    }
}
