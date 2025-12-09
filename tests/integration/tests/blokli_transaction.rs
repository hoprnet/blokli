use blokli_integration_tests::{init_tracing, run_blokli_transaction_test};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn blokli_transaction_test() {
    init_tracing();
    run_blokli_transaction_test()
        .await
        .expect("blokli transaction test failed");
}
