// SPDX-FileCopyrightText: 2026 Mattia Egloff <mattia.egloff@pm.me>
// SPDX-License-Identifier: GPL-3.0-or-later
//! The hardware shell's executors (`vauchi-linux-hw`, ADR-077) hosted for
//! this display shell: one runtime for the NFC, audio and camera sessions,
//! and the pump that hands their events to Core on the main loop. The
//! wiring moves to `linux-app` with ADR-078 step 3; BLE keeps its own host
//! in `ble.rs` for now.
use std::cell::RefCell;
use std::num::NonZeroUsize;
use std::rc::Rc;
use std::sync::{Mutex, MutexGuard, OnceLock, PoisonError};

use gtk4::glib;
use libadwaita as adw;

use vauchi_app::ui::AppEngine;
use vauchi_core::Event;
use vauchi_core::platform::executor::InboxLimits;

use crate::core_ui::contextual_surface::dispatch_platform_event;

/// Audio samples are large: a 48 kHz second of f32 is 192 KB.
pub const INBOX_LIMITS: InboxLimits = InboxLimits {
    operations: NonZeroUsize::new(2).unwrap(),
    events: NonZeroUsize::new(16).unwrap(),
    event_bytes: NonZeroUsize::new(4 << 20).unwrap(),
};

static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();

/// The runtime the hardware sessions run on. A static, not a thread-local:
/// a thread-local host would be disposed after Tokio's own thread-local is
/// gone and abort the process at exit (see `ble.rs`).
pub fn runtime() -> &'static tokio::runtime::Runtime {
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .thread_name("vauchi-hardware")
            .enable_all()
            .build()
            .expect("build the hardware runtime")
    })
}

pub fn lock<T>(slot: &'static Mutex<Option<T>>) -> MutexGuard<'static, Option<T>> {
    slot.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Polls `pop` on the main loop and hands each event to Core until `pop`
/// reports the session gone (`None` from `alive`) or `until` says so.
pub fn pump(
    container: &gtk4::Box,
    app_engine: &Rc<RefCell<AppEngine>>,
    toast_overlay: &adw::ToastOverlay,
    mut pop: impl FnMut() -> Option<Event> + 'static,
    mut alive: impl FnMut() -> bool + 'static,
) {
    let container = container.clone();
    let app_engine = app_engine.clone();
    let toast_overlay = toast_overlay.clone();
    glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
        while alive() {
            let Some(event) = pop() else {
                break;
            };
            dispatch_platform_event(&container, &app_engine, &toast_overlay, event);
        }
        if alive() {
            glib::ControlFlow::Continue
        } else {
            glib::ControlFlow::Break
        }
    });
}
