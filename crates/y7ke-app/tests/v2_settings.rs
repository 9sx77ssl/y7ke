//! V2-A1 settings: boot with empty extras then add one; verify
//! list_bootstraps returns the default first and stays immutable.

use tempfile::TempDir;
use y7ke_app::{AppConfig, AppHandle};
use y7ke_core::settings::{DialMode, Settings, DEFAULT_RELAY_BOOTSTRAP};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn boot_then_add_extra_bootstrap() {
    let dir = TempDir::new().unwrap();
    let app = AppHandle::boot(AppConfig::in_dir(dir.path()))
        .await
        .unwrap();

    // Fresh install: only the hardcoded default appears.
    let initial = app.list_bootstraps().await.unwrap();
    assert_eq!(initial.len(), 1);
    assert_eq!(initial[0].multiaddr, DEFAULT_RELAY_BOOTSTRAP);
    assert!(initial[0].is_default);

    let fake = "/ip4/127.0.0.1/tcp/9999/p2p/12D3KooWAaAaAaAaAaAaAaAaAaAaAaAaAaAaAaAaAaAa";
    app.update_settings(Settings {
        dial_mode: DialMode::default(),
        extra_bootstraps: vec![fake.into()],
    })
    .await
    .unwrap();

    let after = app.list_bootstraps().await.unwrap();
    assert_eq!(after.len(), 2);
    assert_eq!(after[0].multiaddr, DEFAULT_RELAY_BOOTSTRAP);
    assert!(after[0].is_default, "default must stay immutable");
    assert_eq!(after[1].multiaddr, fake);
    assert!(!after[1].is_default);

    app.shutdown().await.ok();
}


#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dial_mode_round_trips_through_settings() {
    let dir = TempDir::new().unwrap();
    let app = AppHandle::boot(AppConfig::in_dir(dir.path()))
        .await
        .unwrap();

    let s0 = app.get_settings().await.unwrap();
    assert_eq!(s0.dial_mode, DialMode::Internet);

    for mode in [DialMode::LanOnly, DialMode::P2p, DialMode::Internet] {
        app.update_settings(Settings {
            dial_mode: mode,
            extra_bootstraps: vec![],
        })
        .await
        .unwrap();
        let got = app.get_settings().await.unwrap();
        assert_eq!(got.dial_mode, mode, "round-trip for {mode:?}");
    }

    app.shutdown().await.ok();
}
