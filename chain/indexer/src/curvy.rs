use blokli_api_types::{CurvyCommittedNote, CurvyEventCursor, CurvyNoteEvent, CurvyPendingNote};
use curvy_bindings::curvy_aggregator_alpha_v2::CurvyAggregatorAlphaV2::CurvyAggregatorAlphaV2Events;

use crate::{
    errors::{CoreEthereumIndexerError, Result},
    state::IndexerEvent,
};

pub fn expand_note_event(
    event: CurvyAggregatorAlphaV2Events,
    block: u64,
    transaction_index: u64,
    log_index: u64,
) -> Result<Vec<IndexerEvent>> {
    match event {
        CurvyAggregatorAlphaV2Events::PendingNotes(event) => {
            let item_count = event.noteIds.len();
            if event.ephemeralKeys[0].len() != item_count
                || event.ephemeralKeys[1].len() != item_count
                || event.viewTags.len() != item_count
                || event.tokens.len() != item_count
                || event.amounts.len() != item_count
                || event.isPlaintext.len() != item_count
            {
                return Err(CoreEthereumIndexerError::ProcessError(
                    "Curvy PendingNotes arrays have inconsistent lengths".to_string(),
                ));
            }

            event
                .noteIds
                .into_iter()
                .enumerate()
                .map(|(raw_item_index, note_id)| {
                    let item_index = u32::try_from(raw_item_index).map_err(|error| {
                        CoreEthereumIndexerError::ProcessError(format!(
                            "Curvy PendingNotes item index does not fit u32: {error}"
                        ))
                    })?;
                    Ok(IndexerEvent::CurvyNote(CurvyNoteEvent::Pending(CurvyPendingNote {
                        cursor: CurvyEventCursor {
                            block,
                            transaction_index,
                            log_index,
                            item_index,
                        },
                        note_id: note_id.to_string(),
                        ephemeral_key: vec![
                            event.ephemeralKeys[0][raw_item_index].to_string(),
                            event.ephemeralKeys[1][raw_item_index].to_string(),
                        ],
                        view_tag: event.viewTags[raw_item_index],
                        token: event.tokens[raw_item_index].to_string(),
                        amount: event.amounts[raw_item_index].to_string(),
                        is_plaintext: event.isPlaintext[raw_item_index],
                    })))
                })
                .collect()
        }
        CurvyAggregatorAlphaV2Events::CommittedNotes(event) => event
            .noteIds
            .into_iter()
            .enumerate()
            .map(|(raw_item_index, note_id)| {
                let item_index = u32::try_from(raw_item_index).map_err(|error| {
                    CoreEthereumIndexerError::ProcessError(format!(
                        "Curvy CommittedNotes item index does not fit u32: {error}"
                    ))
                })?;
                Ok(IndexerEvent::CurvyNote(CurvyNoteEvent::Committed(CurvyCommittedNote {
                    cursor: CurvyEventCursor {
                        block,
                        transaction_index,
                        log_index,
                        item_index,
                    },
                    note_id: note_id.to_string(),
                    batch_index: event.batchIndex.to_string(),
                })))
            })
            .collect(),
        _ => Ok(Vec::new()),
    }
}

#[cfg(test)]
mod tests {
    use curvy_bindings::{
        curvy_aggregator_alpha_v2::CurvyAggregatorAlphaV2::{
            CommittedNotes, CurvyAggregatorAlphaV2Events, PendingNotes,
        },
        exports::alloy::primitives::U256,
    };

    use super::*;

    fn note_events(events: Vec<IndexerEvent>) -> Vec<CurvyNoteEvent> {
        events
            .into_iter()
            .filter_map(|event| match event {
                IndexerEvent::CurvyNote(event) => Some(event),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn expands_pending_notes_into_cursor_addressable_items() {
        let event = CurvyAggregatorAlphaV2Events::PendingNotes(PendingNotes {
            noteIds: vec![U256::from(11), U256::from(12)],
            ephemeralKeys: [
                vec![U256::from(21), U256::from(22)],
                vec![U256::from(31), U256::from(32)],
            ],
            viewTags: vec![41, 42],
            tokens: vec![U256::from(51), U256::from(52)],
            amounts: vec![U256::from(61), U256::from(62)],
            isPlaintext: vec![false, true],
        });

        let events = note_events(expand_note_event(event, 7, 8, 9).expect("valid pending event"));

        assert_eq!(
            events,
            vec![
                CurvyNoteEvent::Pending(CurvyPendingNote {
                    cursor: CurvyEventCursor {
                        block: 7,
                        transaction_index: 8,
                        log_index: 9,
                        item_index: 0,
                    },
                    note_id: "11".to_string(),
                    ephemeral_key: vec!["21".to_string(), "31".to_string()],
                    view_tag: 41,
                    token: "51".to_string(),
                    amount: "61".to_string(),
                    is_plaintext: false,
                }),
                CurvyNoteEvent::Pending(CurvyPendingNote {
                    cursor: CurvyEventCursor {
                        block: 7,
                        transaction_index: 8,
                        log_index: 9,
                        item_index: 1,
                    },
                    note_id: "12".to_string(),
                    ephemeral_key: vec!["22".to_string(), "32".to_string()],
                    view_tag: 42,
                    token: "52".to_string(),
                    amount: "62".to_string(),
                    is_plaintext: true,
                }),
            ]
        );
    }

    #[test]
    fn expands_committed_notes_into_cursor_addressable_items() {
        let event = CurvyAggregatorAlphaV2Events::CommittedNotes(CommittedNotes {
            batchIndex: U256::from(3),
            noteIds: vec![U256::from(11), U256::from(12)],
        });

        let events = note_events(expand_note_event(event, 7, 8, 9).expect("valid committed event"));

        assert_eq!(
            events,
            vec![
                CurvyNoteEvent::Committed(CurvyCommittedNote {
                    cursor: CurvyEventCursor {
                        block: 7,
                        transaction_index: 8,
                        log_index: 9,
                        item_index: 0,
                    },
                    note_id: "11".to_string(),
                    batch_index: "3".to_string(),
                }),
                CurvyNoteEvent::Committed(CurvyCommittedNote {
                    cursor: CurvyEventCursor {
                        block: 7,
                        transaction_index: 8,
                        log_index: 9,
                        item_index: 1,
                    },
                    note_id: "12".to_string(),
                    batch_index: "3".to_string(),
                }),
            ]
        );
    }

    #[test]
    fn rejects_inconsistent_pending_arrays() {
        let event = CurvyAggregatorAlphaV2Events::PendingNotes(PendingNotes {
            noteIds: vec![U256::from(11)],
            ephemeralKeys: [Vec::new(), Vec::new()],
            viewTags: vec![41],
            tokens: vec![U256::from(51)],
            amounts: vec![U256::from(61)],
            isPlaintext: vec![false],
        });

        assert!(expand_note_event(event, 7, 8, 9).is_err());
    }
}
