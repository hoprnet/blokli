use anyhow::Result;
use blokli_integration_tests::fixtures::{IntegrationFixture, integration_fixture as fixture};
use rstest::rstest;
use serial_test::serial;

#[rstest]
#[test_log::test(tokio::test)]
#[serial]
async fn subscribe_channels(fixture: IntegrationFixture) -> Result<()> {
    Ok(())
}

#[rstest]
#[test_log::test(tokio::test)]
#[serial]
async fn subscribe_accounts(fixture: IntegrationFixture) -> Result<()> {
    Ok(())
}

#[rstest]
#[test_log::test(tokio::test)]
#[serial]
async fn subscribe_graph(fixture: IntegrationFixture) -> Result<()> {
    Ok(())
}

#[rstest]
#[test_log::test(tokio::test)]
#[serial]
async fn subscribe_ticket_params(fixture: IntegrationFixture) -> Result<()> {
    Ok(())
}

#[rstest]
#[test_log::test(tokio::test)]
#[serial]
async fn subscribe_safe_deployments(fixture: IntegrationFixture) -> Result<()> {
    Ok(())
}
