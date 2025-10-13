// Ensures CI fast test lane actually sets AETHER_FAST_TEST=1 when EXPECT_FAST=1 is present.
// This prevents silent regressions where fast mode specific skips stop applying.
#[tokio::test]
async fn fast_mode_environment_assert() {
    if std::env::var("EXPECT_FAST").ok().as_deref() != Some("1") {
        // Not a fast-mode assertion context; skip.
        return;
    }
    let val = std::env::var("AETHER_FAST_TEST").ok();
    assert_eq!(val.as_deref(), Some("1"), "EXPECT_FAST=1 but AETHER_FAST_TEST not set to 1 (got {:?})", val);
}
