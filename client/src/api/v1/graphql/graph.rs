use super::{accounts::Account, channels::Channel, schema};

#[derive(cynic::QueryFragment, Debug)]
#[cynic(graphql_type = "SubscriptionRoot")]
pub struct SubscribeGraph {
    pub opened_channel_graph_updated: OpenedChannelsGraphEntry,
}

#[derive(cynic::QueryFragment, Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct OpenedChannelsGraphEntry {
    pub channel: Channel,
    pub destination: Account,
    pub source: Account,
}
