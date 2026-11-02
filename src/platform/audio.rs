// SPDX-FileCopyrightText: 2026 Mattia Egloff <mattia.egloff@pm.me>
// SPDX-License-Identifier: GPL-3.0-or-later
//! Audio proximity through the hardware shell's `AudioSession` (ADR-077):
//! `AudioEmitChallenge` plays Core's pre-encoded samples,
//! `AudioListenForResponse` records and reports `AudioSamplesRecorded`,
//! `AudioStop` ends both. The modulation lives in Core; a failure reaches
//! it as `HardwareError` on "Audio", and the shell shows nothing of its own.
#[cfg(feature = "audio")]
mod inner {
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;
    use std::sync::{Arc, Mutex};

    use libadwaita as adw;

    use vauchi_app::ui::AppEngine;
    use vauchi_core::Event;
    use vauchi_hardware::{AudioSession, CpalAudio};

    use crate::platform::executors::{INBOX_LIMITS, lock, pump, runtime};

    static SESSION: Mutex<Option<AudioSession>> = Mutex::new(None);

    thread_local! {
        static GENERATION: Cell<u64> = const { Cell::new(0) };
        static PUMPING: Cell<bool> = const { Cell::new(false) };
    }

    fn session() -> std::sync::MutexGuard<'static, Option<AudioSession>> {
        let mut slot = lock(&SESSION);
        if slot.is_none() {
            *slot = AudioSession::with_backend(
                Arc::new(CpalAudio),
                runtime().handle().clone(),
                INBOX_LIMITS,
            )
            .ok();
        }
        slot
    }

    fn pop() -> Option<Event> {
        lock(&SESSION).as_mut().and_then(AudioSession::pop)
    }

    fn ensure_pump(
        container: &gtk4::Box,
        app_engine: &Rc<RefCell<AppEngine>>,
        toast_overlay: &adw::ToastOverlay,
    ) {
        if PUMPING.with(Cell::get) {
            return;
        }
        PUMPING.with(|p| p.set(true));
        let generation = GENERATION.with(Cell::get);
        pump(container, app_engine, toast_overlay, pop, move || {
            let alive = GENERATION.with(|g| g.get() == generation);
            if !alive {
                PUMPING.with(|p| p.set(false));
            }
            alive
        });
    }

    /// `AudioEmitChallenge`: play Core's pre-encoded samples.
    pub fn emit_challenge(
        container: &gtk4::Box,
        app_engine: &Rc<RefCell<AppEngine>>,
        toast_overlay: &adw::ToastOverlay,
        samples: Vec<f32>,
        sample_rate: u32,
    ) {
        if let Some(session) = session().as_mut() {
            let _ = session.emit_challenge(samples, sample_rate);
        }
        ensure_pump(container, app_engine, toast_overlay);
    }

    /// `AudioListenForResponse`: record for `timeout_ms`, then report.
    pub fn listen_for_response(
        container: &gtk4::Box,
        app_engine: &Rc<RefCell<AppEngine>>,
        toast_overlay: &adw::ToastOverlay,
        timeout_ms: u64,
    ) {
        if let Some(session) = session().as_mut() {
            let _ = session.listen_for_response(timeout_ms);
        }
        ensure_pump(container, app_engine, toast_overlay);
    }

    /// `AudioStop`: end both, reporting nothing.
    pub fn stop() {
        GENERATION.with(|g| g.set(g.get().wrapping_add(1)));
        if let Some(session) = lock(&SESSION).as_mut() {
            session.stop();
        }
    }
}

#[cfg(feature = "audio")]
pub use inner::*;
