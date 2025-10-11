#![cfg(feature = "ui-egui-test")]
//! Smoke test for the egui adapter lifecycle.
//!
//! This test is intentionally small: it constructs an adapter via
//! `build_adapter`, verifies basic event sending, and exercises the
//! staging belt recall code path. It is gated behind the
//! `ui-egui-test` feature so it does not affect normal consumers.

use std::path::PathBuf;

use moho_ui::{UiEvent, build_adapter};

#[test]
fn adapter_lifecycle_smoke() {
    let (mut adapter, receiver) = build_adapter(None);

    // Adapter should start visible (start menu mode).
    assert!(adapter.is_visible());

    // Construction currently emits an OverlayToggled(true) event so drain
    // any such initialization messages before asserting our explicit
    // actions below.
    while let Ok(ev) = receiver.try_recv() {
        match ev {
            UiEvent::OverlayToggled(true) => continue,
            other => panic!("unexpected event during init drain: {:?}", other),
        }
    }

    // Send a LoadScene and ensure the receiver gets the path.
    adapter.send_load_scene("saves/test.bin");
    match receiver.try_recv() {
        Ok(UiEvent::LoadScene(p)) => assert_eq!(p, PathBuf::from("saves/test.bin")),
        other => panic!("expected LoadScene, got: {:?}", other),
    }

    // Send an Exit event and ensure the receiver sees it.
    adapter.send_exit();
    match receiver.try_recv() {
        Ok(UiEvent::Exit) => {}
        other => panic!("expected Exit, got: {:?}", other),
    }

    // Ensure test helpers and staging belt recall are safe to call.
    adapter.test_set_staging_belt();
    // recall should be idempotent / safe to call multiple times.
    adapter.recall_staging_belt();
    adapter.recall_staging_belt();
}
