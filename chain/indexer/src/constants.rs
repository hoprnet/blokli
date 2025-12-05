use std::time::Duration;

pub const LOGS_SNAPSHOT_DOWNLOADER_MAX_SIZE: u64 = 2 * 1024 * 1024 * 1024; // 2GB max
pub const LOGS_SNAPSHOT_DOWNLOADER_TIMEOUT: Duration = Duration::from_secs(1800); // 30 minutes
pub const LOGS_SNAPSHOT_DOWNLOADER_MAX_RETRIES: u32 = 3;

pub mod topics {
    use alloy::{primitives::B256, sol_types::SolEvent};
    use hopr_bindings::{
        hopr_announcements_events::HoprAnnouncementsEvents::{AddressAnnouncement, KeyBinding, RevokeAnnouncement},
        hopr_channels::HoprChannels::LedgerDomainSeparatorUpdated,
        hopr_channels_events::HoprChannelsEvents::{
            ChannelBalanceDecreased, ChannelBalanceIncreased, ChannelClosed, ChannelOpened, DomainSeparatorUpdated,
            OutgoingChannelClosureInitiated, TicketRedeemed,
        },
        hopr_node_safe_registry_events::HoprNodeSafeRegistryEvents::{DeregisteredNodeSafe, RegisteredNodeSafe},
        hopr_node_stake_factory_events::HoprNodeStakeFactoryEvents::NewHoprNodeStakeModuleForSafe,
        hopr_ticket_price_oracle_events::HoprTicketPriceOracleEvents::TicketPriceUpdated,
        hopr_token::HoprToken::{
            Approval, AuthorizedOperator, Burned, Minted, RevokedOperator, RoleAdminChanged, RoleGranted, RoleRevoked,
            Sent, Transfer,
        },
        hopr_winning_probability_oracle_events::HoprWinningProbabilityOracleEvents::WinProbUpdated,
    };

    pub fn channel() -> Vec<B256> {
        vec![
            ChannelBalanceDecreased::SIGNATURE_HASH,
            ChannelBalanceIncreased::SIGNATURE_HASH,
            ChannelClosed::SIGNATURE_HASH,
            ChannelOpened::SIGNATURE_HASH,
            OutgoingChannelClosureInitiated::SIGNATURE_HASH,
            TicketRedeemed::SIGNATURE_HASH,
            DomainSeparatorUpdated::SIGNATURE_HASH,
            LedgerDomainSeparatorUpdated::SIGNATURE_HASH,
        ]
    }

    pub fn announcement() -> Vec<B256> {
        vec![
            AddressAnnouncement::SIGNATURE_HASH,
            KeyBinding::SIGNATURE_HASH,
            RevokeAnnouncement::SIGNATURE_HASH,
        ]
    }

    pub fn node_safe_registry() -> Vec<B256> {
        vec![
            RegisteredNodeSafe::SIGNATURE_HASH,
            DeregisteredNodeSafe::SIGNATURE_HASH,
            hopr_bindings::hopr_node_safe_registry_events::HoprNodeSafeRegistryEvents::DomainSeparatorUpdated::SIGNATURE_HASH,
        ]
    }

    pub fn ticket_price_oracle() -> Vec<B256> {
        vec![TicketPriceUpdated::SIGNATURE_HASH]
    }

    pub fn winning_prob_oracle() -> Vec<B256> {
        vec![WinProbUpdated::SIGNATURE_HASH]
    }

    pub fn token() -> Vec<B256> {
        vec![
            Transfer::SIGNATURE_HASH,
            Approval::SIGNATURE_HASH,
            AuthorizedOperator::SIGNATURE_HASH,
            Burned::SIGNATURE_HASH,
            Minted::SIGNATURE_HASH,
            RevokedOperator::SIGNATURE_HASH,
            RoleAdminChanged::SIGNATURE_HASH,
            RoleGranted::SIGNATURE_HASH,
            RoleRevoked::SIGNATURE_HASH,
            Sent::SIGNATURE_HASH,
        ]
    }

    pub fn stake_factory() -> Vec<B256> {
        vec![NewHoprNodeStakeModuleForSafe::SIGNATURE_HASH]
    }

    pub fn module_implementation() -> Vec<B256> {
        vec![]
    }
}
