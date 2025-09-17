//! Chain utilities used for testing.
//!
//! This used in unit and integration tests.

#![allow(clippy::too_many_arguments)]

use SafeContract::SafeContractInstance;
use alloy::{
    contract::Result as ContractResult,
    network::{ReceiptResponse, TransactionBuilder},
    primitives::{self, Bytes, U256, aliases, keccak256},
    signers::{Signer, local::PrivateKeySigner},
    sol,
    sol_types::SolCall,
};
use hopr_bindings::{
    hoprchannels::HoprChannels::HoprChannelsInstance,
    hoprtoken::HoprToken::{self, HoprTokenInstance},
};
use hopr_crypto_types::prelude::*;
use hopr_primitive_types::primitives::Address;

use crate::errors::Result as ChainTypesResult;

// define basic safe abi
sol!(
    #![sol(abi)]
    #![sol(rpc)]
    // #[allow(dead_code)]
    contract SafeContract {
        function nonce() view returns (uint256);
        function getTransactionHash( address to, uint256 value, bytes calldata data, uint8 operation, uint256 safeTxGas, uint256 baseGas, uint256 gasPrice, address gasToken, address refundReceiver, uint256 _nonce) public view returns (bytes32);
        function execTransaction(address to, uint256 value, bytes calldata data, uint8 operation, uint256 safeTxGas, uint256 baseGas, uint256 gasPrice, address gasToken, address payable refundReceiver, bytes memory signatures) public returns (bool);
    }
);

lazy_static::lazy_static! {
    static ref MINTER_ROLE_VALUE: primitives::FixedBytes<32> = keccak256("MINTER_ROLE");
}

/// Creates local Anvil instance.
///
/// Used for testing. When block time is given, new blocks are mined periodically.
/// Otherwise, a new block is mined per transaction.
///
/// Uses a fixed mnemonic to make generated accounts deterministic.
pub fn create_anvil(block_time: Option<std::time::Duration>) -> alloy::node_bindings::AnvilInstance {
    let mut anvil = alloy::node_bindings::Anvil::new()
        .mnemonic("gentle wisdom move brush express similar canal dune emotion series because parrot");

    if let Some(bt) = block_time {
        anvil = anvil.block_time(bt.as_secs());
    }

    anvil.spawn()
}

/// Mints specified amount of HOPR tokens to the contract deployer wallet.
/// Assumes that the `hopr_token` contract is associated with a RPC client that also deployed the contract.
/// Returns the block number at which the minting transaction was confirmed.
pub async fn mint_tokens<P, N>(hopr_token: HoprTokenInstance<P, N>, amount: U256) -> ContractResult<Option<u64>>
where
    P: alloy::contract::private::Provider<N>,
    N: alloy::providers::Network,
{
    let deployer = hopr_token
        .provider()
        .get_accounts()
        .await
        .expect("client must have a signer")[0];

    hopr_token
        .grantRole(*MINTER_ROLE_VALUE, deployer)
        .send()
        .await?
        .watch()
        .await?;

    let tx_receipt = hopr_token
        .mint(deployer, amount, Bytes::new(), Bytes::new())
        .send()
        .await?
        .get_receipt()
        .await?;

    Ok(tx_receipt.block_number())
}

/// Creates a transaction that transfers the given `amount` of native tokens to the
/// given destination.
pub fn create_native_transfer<N>(to: Address, amount: U256) -> N::TransactionRequest
where
    N: alloy::providers::Network,
{
    N::TransactionRequest::default().with_to(to.into()).with_value(amount)
}

/// Funds the given wallet address with specified amount of native tokens and HOPR tokens.
/// These must be present in the client's wallet.
pub async fn fund_node<P, N>(
    node: Address,
    native_token: U256,
    hopr_token: U256,
    hopr_token_contract: HoprTokenInstance<P, N>,
) -> ContractResult<()>
where
    P: alloy::contract::private::Provider<N>,
    N: alloy::providers::Network,
{
    let native_transfer_tx = N::TransactionRequest::default()
        .with_to(node.into())
        .with_value(native_token);

    // let native_transfer_tx = Eip1559TransactionRequest::new()
    //     .to(NameOrAddress::Address(node.into()))
    //     .value(native_token);

    let provider = hopr_token_contract.provider();

    provider.send_transaction(native_transfer_tx).await?.watch().await?;

    hopr_token_contract
        .transfer(node.into(), hopr_token)
        .send()
        .await?
        .watch()
        .await?;
    Ok(())
}

/// Funds the channel to the counterparty with the given amount of HOPR tokens.
/// The amount must be present in the wallet of the client.
pub async fn fund_channel<P, N>(
    counterparty: Address,
    hopr_token: HoprTokenInstance<P, N>,
    hopr_channels: HoprChannelsInstance<P, N>,
    amount: U256,
) -> ContractResult<()>
where
    P: alloy::contract::private::Provider<N>,
    N: alloy::providers::Network,
{
    hopr_token
        .approve(*hopr_channels.address(), amount)
        .send()
        .await?
        .watch()
        .await?;

    hopr_channels
        .fundChannel(counterparty.into(), aliases::U96::from(amount))
        .send()
        .await?
        .watch()
        .await?;

    Ok(())
}

/// Funds the channel to the counterparty with the given amount of HOPR tokens, from a different client
/// The amount must be present in the wallet of the client.
pub async fn fund_channel_from_different_client<P, N>(
    counterparty: Address,
    hopr_token_address: Address,
    hopr_channels_address: Address,
    amount: U256,
    new_client: P,
) -> ContractResult<()>
where
    P: alloy::contract::private::Provider<N> + Clone,
    N: alloy::providers::Network,
{
    let hopr_token_with_new_client: HoprTokenInstance<P, N> =
        HoprTokenInstance::new(hopr_token_address.into(), new_client.clone());
    let hopr_channels_with_new_client = HoprChannelsInstance::new(hopr_channels_address.into(), new_client.clone());
    hopr_token_with_new_client
        .approve(hopr_channels_address.into(), amount)
        .send()
        .await?
        .watch()
        .await?;

    hopr_channels_with_new_client
        .fundChannel(counterparty.into(), aliases::U96::from(amount))
        .send()
        .await?
        .watch()
        .await?;

    Ok(())
}

/// Prepare a safe transaction
pub async fn get_safe_tx<P, N>(
    safe_contract: SafeContractInstance<P, N>,
    target: Address,
    inner_tx_data: Bytes,
    wallet: PrivateKeySigner,
) -> ChainTypesResult<N::TransactionRequest>
where
    P: alloy::contract::private::Provider<N>,
    N: alloy::providers::Network,
{
    let nonce = safe_contract.nonce().call().await?;

    let data_hash = safe_contract
        .getTransactionHash(
            target.into(),
            U256::ZERO,
            inner_tx_data.clone(),
            0,
            U256::ZERO,
            U256::ZERO,
            U256::ZERO,
            primitives::Address::default(),
            wallet.address(),
            nonce,
        )
        .call()
        .await?;

    let signed_data_hash = wallet.sign_hash(&data_hash).await?;

    let safe_tx_data = SafeContract::execTransactionCall {
        to: target.into(),
        value: U256::ZERO,
        data: inner_tx_data,
        operation: 0,
        safeTxGas: U256::ZERO,
        baseGas: U256::ZERO,
        gasPrice: U256::ZERO,
        gasToken: primitives::Address::default(),
        refundReceiver: wallet.address(),
        signatures: Bytes::from(signed_data_hash.as_bytes()),
    }
    .abi_encode();

    // Outer tx payload: execute as safe tx
    let safe_tx = N::TransactionRequest::default()
        .with_to(*safe_contract.address())
        .with_input(safe_tx_data);

    Ok(safe_tx)
}

/// Send a Safe transaction to the token contract, to approve channels on behalf of safe.
pub async fn approve_channel_transfer_from_safe<P, N>(
    provider: P,
    safe_address: Address,
    token_address: Address,
    channel_address: Address,
    deployer: &ChainKeypair, // also node address
) -> ContractResult<()>
where
    P: alloy::contract::private::Provider<N> + Clone,
    N: alloy::providers::Network,
{
    // Inner tx payload: include node to the module
    let inner_tx_data = HoprToken::approveCall {
        spender: channel_address.into(),
        value: U256::MAX,
    }
    .abi_encode();

    let safe_contract = SafeContract::new(safe_address.into(), provider.clone());
    let wallet = PrivateKeySigner::from_slice(deployer.secret().as_ref()).expect("failed to construct wallet");
    let safe_tx = get_safe_tx(safe_contract, token_address, inner_tx_data.into(), wallet)
        .await
        .unwrap();

    provider.send_transaction(safe_tx).await?.watch().await?;

    Ok(())
}
