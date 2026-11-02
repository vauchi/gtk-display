// SPDX-FileCopyrightText: 2026 Mattia Egloff <mattia.egloff@pm.me>
// SPDX-License-Identifier: GPL-3.0-or-later
//! NFC through the hardware shell's `NfcSession` (ADR-077): `NfcActivate`
//! waits for a tag on the PC/SC reader and transmits Core's frames,
//! `NfcSendApdu` reuses the tag, `NfcDeactivate` ends the wait or the
//! connection. Every response reaches Core as an event on the main loop.
#[cfg(feature = "nfc")]
mod inner {
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;
    use std::sync::{Arc, Mutex};

    use libadwaita as adw;

    use vauchi_app::ui::AppEngine;
    use vauchi_core::Event;
    use vauchi_hardware::{NfcSession, PcscReader};

    use crate::platform::executors::{INBOX_LIMITS, lock, pump, runtime};

    static SESSION: Mutex<Option<NfcSession>> = Mutex::new(None);

    thread_local! {
        static GENERATION: Cell<u64> = const { Cell::new(0) };
    }

    fn session() -> std::sync::MutexGuard<'static, Option<NfcSession>> {
        let mut slot = lock(&SESSION);
        if slot.is_none() {
            *slot = NfcSession::with_backend(
                Arc::new(PcscReader),
                runtime().handle().clone(),
                INBOX_LIMITS,
            )
            .ok();
        }
        slot
    }

    fn pop() -> Option<Event> {
        lock(&SESSION).as_mut().and_then(NfcSession::pop)
    }

    /// `NfcActivate`: wait for a tag, then transmit Core's frames.
    pub fn activate(
        container: &gtk4::Box,
        app_engine: &Rc<RefCell<AppEngine>>,
        toast_overlay: &adw::ToastOverlay,
        apdus: Vec<Vec<u8>>,
    ) {
        let generation = GENERATION.with(|g| {
            g.set(g.get().wrapping_add(1));
            g.get()
        });
        let started = session()
            .as_mut()
            .is_some_and(|session| session.activate(apdus).is_ok());
        if !started {
            crate::core_ui::contextual_surface::dispatch_platform_event(
                container,
                app_engine,
                toast_overlay,
                Event::HardwareUnavailable {
                    transport: "NFC".into(),
                },
            );
            return;
        }
        pump(container, app_engine, toast_overlay, pop, move || {
            GENERATION.with(|g| g.get() == generation)
        });
    }

    /// `NfcSendApdu`: transmit Core's frames on the connected tag.
    pub fn send(apdus: Vec<Vec<u8>>) {
        if let Some(session) = session().as_mut() {
            let _ = session.send(apdus);
        }
    }

    /// `NfcDeactivate`.
    pub fn deactivate() {
        GENERATION.with(|g| g.set(g.get().wrapping_add(1)));
        if let Some(session) = lock(&SESSION).as_mut() {
            session.deactivate();
        }
    }
}

#[cfg(feature = "nfc")]
pub use inner::*;
