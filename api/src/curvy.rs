use blokli_api_types::{
    CurvyCommitmentGasCostUpdate, CurvyCommitmentGasFeeRootUpdate, CurvyCommittedNote, CurvyCommittedNullifier,
    CurvyEventPosition, CurvyGasFees, CurvyPendingNote, CurvyTokenRegistration, Hex32, UInt64, UInt256,
};
use blokli_db_entity::{
    curvy_commitment_gas_cost, curvy_commitment_gas_fee_root, curvy_committed_note, curvy_committed_nullifier,
    curvy_pending_note, curvy_token_registration,
};
use hopr_bindings::exports::alloy::primitives::U256;
use hopr_types::{crypto::types::Hash, primitive::prelude::Address, primitive::traits::ToHex};

use crate::errors;

fn bytes32(value: &[u8], field: &str) -> async_graphql::Result<[u8; 32]> {
    value
        .try_into()
        .map_err(|_| errors::graphql_query_error("decode Curvy event", format!("{field} is not 32 bytes")))
}

fn uint256(value: &[u8], field: &str) -> async_graphql::Result<UInt256> {
    Ok(UInt256(U256::from_be_bytes(bytes32(value, field)?).to_string()))
}

fn position(
    chain_tx_hash: &[u8],
    block: i64,
    transaction_index: i64,
    log_index: i64,
) -> async_graphql::Result<CurvyEventPosition> {
    Ok(CurvyEventPosition {
        transaction_hash: Hex32(Hash::from(bytes32(chain_tx_hash, "chain_tx_hash")?).to_hex()),
        block: UInt64(u64::try_from(block).map_err(|error| errors::graphql_query_error("decode Curvy event", error))?),
        transaction_index: UInt64(
            u64::try_from(transaction_index)
                .map_err(|error| errors::graphql_query_error("decode Curvy event", error))?,
        ),
        log_index: UInt64(
            u64::try_from(log_index).map_err(|error| errors::graphql_query_error("decode Curvy event", error))?,
        ),
    })
}

pub fn pending_note(model: curvy_pending_note::Model) -> async_graphql::Result<CurvyPendingNote> {
    Ok(CurvyPendingNote {
        note_id: uint256(&model.note_id, "note_id")?,
        ephemeral_key: vec![
            uint256(&model.ephemeral_key_x, "ephemeral_key_x")?,
            uint256(&model.ephemeral_key_y, "ephemeral_key_y")?,
        ],
        view_tag: i32::try_from(model.view_tag)
            .map_err(|error| errors::graphql_query_error("decode Curvy event", error))?,
        token_id: uint256(&model.token_id, "token_id")?,
        amount: uint256(&model.amount, "amount")?,
        is_plaintext: model.is_plaintext,
        position: position(
            &model.chain_tx_hash,
            model.published_block,
            model.published_tx_index,
            model.published_log_index,
        )?,
    })
}

pub fn committed_note(model: curvy_committed_note::Model) -> async_graphql::Result<CurvyCommittedNote> {
    Ok(CurvyCommittedNote {
        batch_index: uint256(&model.batch_index, "batch_index")?,
        note_id: uint256(&model.note_id, "note_id")?,
        position: position(
            &model.chain_tx_hash,
            model.published_block,
            model.published_tx_index,
            model.published_log_index,
        )?,
    })
}

pub fn committed_nullifier(model: curvy_committed_nullifier::Model) -> async_graphql::Result<CurvyCommittedNullifier> {
    Ok(CurvyCommittedNullifier {
        batch_index: uint256(&model.batch_index, "batch_index")?,
        nullifier: uint256(&model.nullifier, "nullifier")?,
        position: position(
            &model.chain_tx_hash,
            model.published_block,
            model.published_tx_index,
            model.published_log_index,
        )?,
    })
}

pub fn commitment_gas_fee_root(
    model: curvy_commitment_gas_fee_root::Model,
) -> async_graphql::Result<CurvyCommitmentGasFeeRootUpdate> {
    Ok(CurvyCommitmentGasFeeRootUpdate {
        root: uint256(&model.root, "root")?,
        position: position(
            &model.chain_tx_hash,
            model.published_block,
            model.published_tx_index,
            model.published_log_index,
        )?,
    })
}

pub fn token_registration(model: curvy_token_registration::Model) -> async_graphql::Result<CurvyTokenRegistration> {
    let token_address: [u8; 20] = model
        .token_address
        .as_slice()
        .try_into()
        .map_err(|_| errors::graphql_query_error("decode Curvy event", "token_address is not 20 bytes"))?;
    Ok(CurvyTokenRegistration {
        token_address: Address::from(token_address).to_hex(),
        token_id: uint256(&model.token_id, "token_id")?,
        position: position(
            &model.chain_tx_hash,
            model.published_block,
            model.published_tx_index,
            model.published_log_index,
        )?,
    })
}

pub fn commitment_gas_cost(
    model: curvy_commitment_gas_cost::Model,
) -> async_graphql::Result<CurvyCommitmentGasCostUpdate> {
    Ok(CurvyCommitmentGasCostUpdate {
        gas_fees: CurvyGasFees {
            token_id: uint256(&model.token_id, "token_id")?,
            portal_deployment: uint256(&model.portal_deployment, "portal_deployment")?,
            pending_note_commitment: uint256(&model.pending_note_commitment, "pending_note_commitment")?,
            withdrawal: uint256(&model.withdrawal, "withdrawal")?,
        },
        root: uint256(&model.root, "root")?,
        position: position(
            &model.chain_tx_hash,
            model.published_block,
            model.published_tx_index,
            model.published_log_index,
        )?,
    })
}
