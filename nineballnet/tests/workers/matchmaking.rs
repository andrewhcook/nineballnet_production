use loco_rs::{bgworker::BackgroundWorker, testing::prelude::*}; // Changed bgworker to bg
use nineballnet::{
    app::App,
    workers::matchmaking::{MatchmakingWorker, MatchmakingWorkerArgs},
};
use serial_test::serial;
use uuid::Uuid;

#[tokio::test]
#[serial]
async fn test_run_matchmaking_worker() {
    let boot = boot_test::<App>().await.unwrap();

    // 1. Define a dummy player_id for the test
    let player_id = Uuid::new_v4();

    // 2. Execute the worker
    // Note: ensure your config/test.yaml has workers: mode: ForegroundBlocking
    assert!(
        MatchmakingWorker::perform_later(
            &boot.app_context, 
            MatchmakingWorkerArgs { player_id } 
        )
        .await
        .is_ok()
    );

    // 3. Optional: Verify that a 'ready' match record actually exists in the DB now
    // This is the "intellectual honesty" part of testing!
}