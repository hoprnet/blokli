use std::{str::FromStr, time::Duration};

use anyhow::{Context, Result};
use blokli_client::api::{
    AccountSelector, BlokliQueryClient, ChannelFilter, ChannelSelector, RedeemedStatsSelector, SafeSelector,
    types::{ChannelStatus, TransactionStatus},
};
use blokli_integration_tests::{
    constants::parsed_safe_balance,
    fixtures::{IntegrationFixture, integration_fixture as fixture},
};
use hex::{FromHex, ToHex};
use hopr_bindings::exports::alloy::primitives::{Address, U256};
use hopr_types::{
    crypto::keypairs::Keypair,
    internal::channels::generate_channel_id,
    primitive::prelude::{HoprBalance, XDaiBalance},
};
use rstest::*;
use serial_test::serial;
use tokio::time::sleep;

#[rstest]
#[test_log::test(tokio::test)]
#[serial]
/// deploy a safe, publish an announcement using the safe module, and verify that the account count
/// increases by one.
async fn count_accounts_matches_deployed_accounts(#[future(awt)] fixture: IntegrationFixture) -> Result<()> {
    let [account] = fixture.sample_accounts::<1>();

    fixture.deploy_safe_and_announce(account, parsed_safe_balance()).await?;

    assert_eq!(
        fixture
            .client()
            .count_accounts(AccountSelector::Address(account.address.into()))
            .await?,
        1
    );
    Ok(())
}

#[rstest]
#[test_log::test(tokio::test)]
#[serial]
/// deploy a safe, publish an announcement using the safe module, and verify that the account is
/// found upon querying.
async fn query_accounts(#[future(awt)] fixture: IntegrationFixture) -> Result<()> {
    let [account] = fixture.sample_accounts::<1>();

    let deployed_safe = fixture.deploy_safe_and_announce(account, parsed_safe_balance()).await?;

    let found_accounts = fixture
        .client()
        .query_accounts(AccountSelector::Address(account.address.into()))
        .await?;

    assert_eq!(found_accounts.len(), 1);
    assert_eq!(found_accounts[0].chain_key.to_lowercase(), account.address.to_string());
    assert_eq!(
        found_accounts[0].packet_key.to_lowercase(),
        account.offchain_keypair().public().encode_hex::<String>()
    );
    assert_eq!(
        found_accounts[0].safe_address.clone().map(|a| a.to_lowercase()),
        Some(deployed_safe.address.to_lowercase())
    );
    assert_eq!(
        found_accounts[0].chain_key.to_lowercase(),
        deployed_safe.chain_key.to_lowercase(),
    );

    Ok(())
}

#[rstest]
#[test_log::test(tokio::test)]
#[serial]
/// query the native balance of an EOA via blokli and via direct RPC, and verify that both match
/// and that the returned format is correct.
async fn query_native_balance(#[future(awt)] fixture: IntegrationFixture) -> Result<()> {
    let [account] = fixture.sample_accounts::<1>();

    let blokli_balance = fixture
        .client()
        .query_native_balance(account.to_alloy_address().as_ref())
        .await?;
    let rpc_balance = fixture.rpc().get_balance(&account.address).await?;

    let parsed_blokli_balance =
        XDaiBalance::from_str(blokli_balance.balance.0.as_ref()).expect("failed to parse blokli balance");
    let parsed_rpc_balance =
        XDaiBalance::from_str(&format!("{rpc_balance} wei xDai")).expect("failed to parse rpc balance");

    assert_eq!(parsed_blokli_balance, parsed_rpc_balance);
    Ok(())
}

#[rstest]
#[test_log::test(tokio::test)]
/// query the token (wxHOPR) balance of an EOA via blokli and verify that the format is correct.
async fn query_token_balance_of_eoa(#[future(awt)] fixture: IntegrationFixture) -> Result<()> {
    let [account] = fixture.sample_accounts::<1>();

    let blokli_balance = fixture
        .client()
        .query_token_balance(account.to_alloy_address().as_ref())
        .await?;

    let _: HoprBalance = blokli_balance
        .balance
        .0
        .parse()
        .expect("failed to parse blokli token balance");

    Ok(())
}

#[rstest]
#[test_log::test(tokio::test)]
#[serial]
/// deploy a safe, query the token (wxHOPR) balance of the safe via blokli and verify that
/// format is correct.
async fn query_token_balance_and_allowance_of_safe(#[future(awt)] fixture: IntegrationFixture) -> Result<()> {
    let [account] = fixture.sample_accounts::<1>();

    let maybe_safe = fixture
        .client()
        .query_safe(SafeSelector::ChainKey(account.to_alloy_address().into()))
        .await?;

    let safe = match maybe_safe {
        Some(safe) => safe,
        None => {
            fixture.deploy_or_get_safe(account, parsed_safe_balance()).await?;
            tokio::time::sleep(Duration::from_secs(8)).await; // dummy wait for the safe to be indexed
            fixture
                .client()
                .query_safe(SafeSelector::ChainKey(account.to_alloy_address().into()))
                .await?
                .expect("Safe not found")
        }
    };

    let safe_address = Address::from_str(&safe.address)?;

    let blokli_balance = fixture.client().query_token_balance(safe_address.as_ref()).await?;

    let _: HoprBalance = blokli_balance
        .balance
        .0
        .parse()
        .expect("failed to parse blokli token balance");

    let _: HoprBalance = fixture
        .client()
        .query_safe_allowance(&safe_address)
        .await?
        .allowance
        .0
        .parse()
        .expect("failed to parse retrieved allowance amount");

    Ok(())
}

#[rstest]
#[test_log::test(tokio::test)]
#[serial]
/// deploys a safe and verifies that the queried Safe includes the indexed owners list.
async fn query_safe_returns_indexed_owners(#[future(awt)] fixture: IntegrationFixture) -> Result<()> {
    let [account] = fixture.sample_accounts::<1>();
    let deployed_safe = fixture.deploy_or_get_safe(account, parsed_safe_balance()).await?;

    sleep(Duration::from_secs(8)).await;

    let safe = fixture
        .client()
        .query_safe(SafeSelector::Owner(account.to_alloy_address().into()))
        .await?
        .context("deployed safe not found")?;

    assert_eq!(safe.address.to_lowercase(), deployed_safe.address.to_lowercase());
    assert!(
        safe.owners
            .iter()
            .any(|owner| owner.eq_ignore_ascii_case(&account.address.to_string())),
        "queried safe owners should contain the deploying account"
    );

    Ok(())
}

#[rstest]
#[test_log::test(tokio::test)]
#[serial]
async fn query_safe_redeemed_stats_after_ticket_redeem(#[future(awt)] fixture: IntegrationFixture) -> Result<()> {
    let channel_amount: HoprBalance = "5 wei wxHOPR".parse().expect("failed to parse channel amount");
    let ticket_amount: HoprBalance = "1 wei wxHOPR".parse().expect("failed to parse ticket amount");

    let [src, dst] = fixture.sample_accounts::<2>();
    let expected_id = generate_channel_id(&src.address, &dst.address);
    let channel_selector = ChannelSelector {
        filter: Some(ChannelFilter::ChannelId(expected_id.into())),
        status: Some(ChannelStatus::Open),
        ..Default::default()
    };

    let src_safe = fixture.deploy_safe_and_announce(src, parsed_safe_balance()).await?;
    let dst_safe = fixture.deploy_safe_and_announce(dst, parsed_safe_balance()).await?;

    fixture.approve(src, channel_amount, &src_safe.module_address).await?;
    sleep(Duration::from_secs(8)).await;

    fixture
        .open_channel(src, dst, channel_amount, &src_safe.module_address, None)
        .await?;

    sleep(Duration::from_secs(8)).await;

    let channels = fixture.client().query_channels(channel_selector.clone()).await?;
    let channel = channels
        .channels
        .first()
        .context("opened channel not found after opening channel")?;

    let ticket_index: u64 = channel.ticket_index.0.parse()?;
    let channel_epoch = u32::try_from(channel.epoch).context("channel epoch is negative or invalid")?;

    let dst_safe_address = Address::from_str(&dst_safe.address)?;
    let initial_stats = fixture
        .client()
        .query_redeemed_stats(RedeemedStatsSelector::SafeAddress(dst_safe_address.into()))
        .await?;

    fixture
        .redeem_ticket(
            src,
            dst,
            ticket_amount,
            &dst_safe.module_address,
            ticket_index,
            channel_epoch,
        )
        .await?;

    sleep(Duration::from_secs(8)).await;

    let stats = fixture
        .client()
        .query_redeemed_stats(RedeemedStatsSelector::SafeAddress(dst_safe_address.into()))
        .await?;

    fixture
        .initiate_outgoing_channel_closure(src, dst, &src_safe.module_address)
        .await?;

    assert_eq!(
        initial_stats.redemption_count.0.parse::<u64>()? + 1,
        stats.redemption_count.0.parse::<u64>()?
    );
    assert_eq!(
        initial_stats.redeemed_amount.0.parse::<HoprBalance>()? + ticket_amount,
        stats.redeemed_amount.0.parse::<HoprBalance>()?
    );

    Ok(())
}

#[rstest]
#[test_log::test(tokio::test)]
#[serial]
/// verifies that the transaction count of a given account increases by one after submitting a transaction.
async fn query_transaction_count(#[future(awt)] fixture: IntegrationFixture) -> Result<()> {
    let [sender, recipient] = fixture.sample_accounts::<2>();
    let tx_value = U256::from(1_000_000u64);
    let nonce = fixture.rpc().transaction_count(&sender.address).await?;

    let signed_bytes = Vec::from_hex(
        fixture
            .build_raw_tx(tx_value, sender, recipient, nonce)
            .await?
            .trim_start_matches("0x"),
    )
    .expect("failed to decode raw tx");

    let before_count = fixture
        .client()
        .query_transaction_count(sender.to_alloy_address().as_ref())
        .await?;

    let tx_id = fixture.submit_and_track_tx(&signed_bytes).await?;
    loop {
        let tx = fixture.client().query_transaction_status(tx_id.clone()).await?;
        if tx.status == TransactionStatus::Confirmed {
            break;
        }
        sleep(Duration::from_secs(2)).await;
    }

    let after_count = fixture
        .client()
        .query_transaction_count(sender.to_alloy_address().as_ref())
        .await?;

    assert_eq!(after_count, before_count + 1);

    Ok(())
}

#[rstest]
#[test_log::test(tokio::test)]
#[serial]
/// deploys two safes from different EOAs, publish announcements using the safe modules, open a channel
/// between the EOAs, and verifies that the channel count increases by one
async fn count_and_query_channels(#[future(awt)] fixture: IntegrationFixture) -> Result<()> {
    let [src, dst] = fixture.sample_accounts::<2>();
    let expected_id = generate_channel_id(&src.address, &dst.address);

    let channel_selector = ChannelSelector {
        filter: Some(ChannelFilter::ChannelId(expected_id.into())),
        status: Some(ChannelStatus::Open),
        ..Default::default()
    };

    let amount: HoprBalance = "1 wei wxHOPR".parse().expect("failed to parse amount");

    // Deploy safes for both parties
    let src_safe = fixture.deploy_safe_and_announce(src, parsed_safe_balance()).await?;
    let dst_safe = fixture.deploy_safe_and_announce(dst, parsed_safe_balance()).await?;

    // Set allowance
    fixture.approve(src, amount, &src_safe.module_address).await?;

    sleep(Duration::from_secs(8)).await;

    // Create the channel
    fixture
        .open_channel(src, dst, amount, &src_safe.module_address, None)
        .await?;

    sleep(Duration::from_secs(8)).await;

    let after_count = fixture
        .client()
        .query_channel_stats(channel_selector.clone())
        .await?
        .count;
    let queried_channels = fixture.client().query_channels(channel_selector.clone()).await?;

    assert_eq!(after_count, 1);
    assert_eq!(queried_channels.channels.len(), 1);
    assert_eq!(
        queried_channels.channels[0].concrete_channel_id.to_lowercase(),
        hex::encode(expected_id).to_lowercase()
    );
    assert_eq!(
        queried_channels.channels[0]
            .balance
            .0
            .parse::<HoprBalance>()
            .context("failed to parse channel balance from channels query")?,
        amount
    );

    let src_safe_selector = ChannelSelector {
        safe_address: Some(Address::from_str(&src_safe.address)?.into()),
        status: Some(ChannelStatus::Open),
        ..Default::default()
    };
    let src_safe_count = fixture
        .client()
        .query_channel_stats(src_safe_selector.clone())
        .await?
        .count;
    let src_safe_channels = fixture.client().query_channels(src_safe_selector).await?;
    assert_eq!(src_safe_count, 1);
    assert_eq!(src_safe_channels.channels.len(), 1);

    let dst_safe_selector = ChannelSelector {
        safe_address: Some(Address::from_str(&dst_safe.address)?.into()),
        status: Some(ChannelStatus::Open),
        ..Default::default()
    };
    let dst_safe_count = fixture
        .client()
        .query_channel_stats(dst_safe_selector.clone())
        .await?
        .count;
    let dst_safe_channels = fixture.client().query_channels(dst_safe_selector).await?;
    assert_eq!(dst_safe_count, 0);
    assert!(dst_safe_channels.channels.is_empty());

    fixture
        .initiate_outgoing_channel_closure(src, dst, &src_safe.module_address)
        .await?;

    sleep(Duration::from_secs(8)).await;

    let count_after_closure = fixture.client().query_channel_stats(channel_selector).await?.count;

    assert_eq!(count_after_closure, 0);

    Ok(())
}

#[rstest]
#[test_log::test(tokio::test)]
#[serial]
/// deploys two safes, opens a channel, and verifies that `channelStats` returns the correct count
/// and total balance across various filter combinations (channel id, source safe, destination safe,
/// and unfiltered).
async fn channel_stats_count_and_balance(#[future(awt)] fixture: IntegrationFixture) -> Result<()> {
    let [src, dst] = fixture.sample_accounts::<2>();
    let expected_id = generate_channel_id(&src.address, &dst.address);

    let amount: HoprBalance = "1 wei wxHOPR".parse().expect("failed to parse amount");

    let src_safe = fixture.deploy_safe_and_announce(src, parsed_safe_balance()).await?;
    let dst_safe = fixture.deploy_safe_and_announce(dst, parsed_safe_balance()).await?;

    fixture.approve(src, amount, &src_safe.module_address).await?;

    sleep(Duration::from_secs(8)).await;

    fixture
        .open_channel(src, dst, amount, &src_safe.module_address, None)
        .await?;

    sleep(Duration::from_secs(8)).await;

    // Filter by concrete channel id
    let by_id = fixture
        .client()
        .query_channel_stats(ChannelSelector {
            filter: Some(ChannelFilter::ChannelId(expected_id.into())),
            status: Some(ChannelStatus::Open),
            ..Default::default()
        })
        .await?;
    assert_eq!(by_id.count, 1);
    assert_eq!(
        by_id
            .balance
            .0
            .parse::<HoprBalance>()
            .context("failed to parse balance from channel_stats (by id)")?,
        amount
    );

    // Filter by source safe — should include the open channel
    let by_src_safe = fixture
        .client()
        .query_channel_stats(ChannelSelector {
            safe_address: Some(Address::from_str(&src_safe.address)?.into()),
            status: Some(ChannelStatus::Open),
            ..Default::default()
        })
        .await?;
    assert_eq!(by_src_safe.count, 1);
    assert_eq!(
        by_src_safe
            .balance
            .0
            .parse::<HoprBalance>()
            .context("failed to parse balance from channel_stats (src safe)")?,
        amount
    );

    // Filter by destination safe — should be empty (dst has no outgoing open channels)
    let by_dst_safe = fixture
        .client()
        .query_channel_stats(ChannelSelector {
            safe_address: Some(Address::from_str(&dst_safe.address)?.into()),
            status: Some(ChannelStatus::Open),
            ..Default::default()
        })
        .await?;

    assert_eq!(by_dst_safe.count, 0);
    assert_eq!(
        by_dst_safe
            .balance
            .0
            .parse::<HoprBalance>()
            .context("failed to parse balance from channel_stats (dst safe)")?,
        HoprBalance::zero()
    );

    // Unfiltered — at least one open channel, total balance >= amount
    let unfiltered = fixture
        .client()
        .query_channel_stats(ChannelSelector {
            status: Some(ChannelStatus::Open),
            ..Default::default()
        })
        .await?;
    assert!(unfiltered.count >= 1);
    assert!(
        unfiltered
            .balance
            .0
            .parse::<HoprBalance>()
            .context("failed to parse balance from channel_stats (unfiltered)")?
            >= amount
    );

    fixture
        .initiate_outgoing_channel_closure(src, dst, &src_safe.module_address)
        .await?;

    Ok(())
}

#[rstest]
#[test_log::test(tokio::test)]
#[serial]
/// deploys a safe and verifies owner filtering semantics for `safesBalance`.
async fn query_safes_balance_for_owner(#[future(awt)] fixture: IntegrationFixture) -> Result<()> {
    let [owner] = fixture.sample_accounts::<1>();
    let safe = fixture.deploy_safe_and_announce(owner, parsed_safe_balance()).await?;
    sleep(Duration::from_secs(8)).await;

    let safes_count_and_balance = fixture
        .client()
        .query_safes_balance(Some(owner.to_alloy_address().into()))
        .await?;

    let safe_balance = fixture
        .client()
        .query_token_balance(Address::from_str(&safe.address)?.as_ref())
        .await?;

    assert_eq!(safes_count_and_balance.count, 1);
    assert_eq!(safes_count_and_balance.balance, safe_balance.balance);
    assert!(
        safes_count_and_balance.balance.0.parse::<HoprBalance>()? > HoprBalance::zero(),
        "owner safes' balance should be greater than zero for deployed funded safe"
    );

    let owner_with_no_safe_counts_and_balance = fixture.client().query_safes_balance(Some([0u8; 20])).await?;

    assert_eq!(owner_with_no_safe_counts_and_balance.count, 0);
    assert_eq!(
        owner_with_no_safe_counts_and_balance
            .balance
            .0
            .parse::<HoprBalance>()
            .context("failed to parse empty-owner safes balance")?,
        HoprBalance::zero()
    );

    Ok(())
}

#[rstest]
#[test_log::test(tokio::test)]
/// verifies that the chain infos provided by blokli and the RPC match.
async fn chain_info_contains_correct_chain_id_and_key_binding_fee(
    #[future(awt)] fixture: IntegrationFixture,
) -> Result<()> {
    let chain = fixture.client().query_chain_info().await?;
    let chain_id = fixture.rpc().chain_id().await?;

    assert_eq!(chain.chain_id as u64, chain_id);
    assert_eq!(chain.key_binding_fee.0, "0.01 wxHOPR");

    Ok(())
}

#[rstest]
#[test_log::test(tokio::test)]
/// verifies that the version string returned by blokli can be parsed as a semver.
async fn query_version_should_be_parsable(#[future(awt)] fixture: IntegrationFixture) -> Result<()> {
    let version = fixture.client().query_version().await?;
    let _parsed_version = semver::Version::parse(&version).expect("failed to parse version as semver");
    Ok(())
}

#[rstest]
#[test_log::test(tokio::test)]
/// verifies that the health status returned by blokli is "ok".
async fn query_health_should_be_ok(#[future(awt)] fixture: IntegrationFixture) -> Result<()> {
    assert_eq!(fixture.client().query_health().await?, "ok");
    Ok(())
}
