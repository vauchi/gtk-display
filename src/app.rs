// SPDX-FileCopyrightText: 2026 Mattia Egloff <mattia.egloff@pm.me>
// SPDX-License-Identifier: GPL-3.0-or-later

//! The engine wiring: Core's engine and the hardware the GTK shell runs
//! around (`crate::shell`). Moves to linux-app in ADR-078 step 4d.4.
use std::rc::Rc;

use vauchi_app::i18n;
use vauchi_app::ui::AppEngine;

use crate::core_ui::engine_link::EngineHandle;
use crate::core_ui::hardware_link::{StartHardware, StartedHardware};
use crate::engine::CoreEngine;
use crate::platform;

/// gvauchi with linux-hw's `LinuxHardware`, as linux-app runs it.
pub fn run() {
    run_with(Box::new(start_linux_hardware));
}

/// The GTK app with the hardware the caller starts (ADR-078 step 3b) and
/// the engine this crate still builds until the wiring moves to linux-app
/// (step 4d.4).
pub fn run_with(start_hardware: StartHardware) {
    if let Some(resource_dir) = std::env::var_os("VAUCHI_LOCALES_DIR") {
        let _ = i18n::init(std::path::Path::new(&resource_dir));
    }
    crate::shell::run(Box::new(start_core_engine), start_hardware);
}

/// Core's engine for the window: storage, the test identity if asked, the
/// system locale, content updates, then the network-cost watcher.
pub fn start_core_engine() -> EngineHandle {
    let mut vauchi = platform::init::init_vauchi().expect("Failed to initialize Vauchi backend");
    maybe_seed_test_identity(&mut vauchi);
    let mut app_engine = AppEngine::new(vauchi);
    crate::locale::report_system_locale(&mut app_engine, |name| std::env::var(name).ok());
    // CDN languages, themes and social networks: Core runs the update cycle
    // and announces applied content as a Vauchi event, which the shell
    // re-renders on (vauchi/private#733 D10).
    if let Some(data_dir) = app_engine
        .vauchi()
        .config()
        .storage_path
        .parent()
        .map(std::path::Path::to_path_buf)
    {
        app_engine.start_content_updates(&data_dir, vauchi_app::ui::ContentRunOptions::default());
    }
    app_engine.bootstrap();
    let engine_handle: EngineHandle = Rc::new(CoreEngine::new(app_engine));
    platform::network_cost::watch(&engine_handle);
    engine_handle
}

/// Creates a test identity so the app skips onboarding and lands on home.
///
/// Triggered by `--reset-for-testing` CLI flag (consumed before GTK init)
/// or `VAUCHI_TEST_SEED=1` env var (used by CI conftest to avoid GTK
/// argument parser rejecting the unknown flag).
fn maybe_seed_test_identity(vauchi: &mut vauchi_core::api::Vauchi) {
    let cli_flag = crate::shell::reset_for_testing_requested();
    let env_flag = std::env::var("VAUCHI_TEST_SEED").as_deref() == Ok("1");
    if !cli_flag && !env_flag {
        return;
    }
    if vauchi.has_identity() {
        eprintln!("[vauchi] --reset-for-testing: identity already exists, skipping");
        return;
    }
    match vauchi.create_identity("Test User") {
        Ok(()) => eprintln!("[vauchi] --reset-for-testing: test identity created"),
        Err(e) => eprintln!("[vauchi] --reset-for-testing: failed to create identity: {e}"),
    }
}

/// linux-hw's hardware shell; without any hardware feature, or when the
/// shell cannot start, there is none and every command stays here.
pub fn start_linux_hardware() -> Option<StartedHardware> {
    #[cfg(any(
        feature = "ble",
        feature = "nfc",
        feature = "audio",
        feature = "camera"
    ))]
    {
        let hardware = match vauchi_hardware::LinuxHardware::start() {
            Ok(hardware) => hardware,
            Err(error) => {
                eprintln!("[hardware] linux-hw did not start: {error}");
                return None;
            }
        };
        let link = platform::linux_hardware_link::LinuxHardwareLink::new(hardware);
        let capabilities = Some(link.report());
        Some(StartedHardware {
            link: Rc::new(link),
            capabilities,
        })
    }
    #[cfg(not(any(
        feature = "ble",
        feature = "nfc",
        feature = "audio",
        feature = "camera"
    )))]
    {
        None
    }
}
