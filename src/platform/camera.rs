// SPDX-FileCopyrightText: 2026 Mattia Egloff <mattia.egloff@pm.me>
// SPDX-License-Identifier: GPL-3.0-or-later
//! QR scanning through the hardware shell's `CameraSession` (ADR-077): the
//! capture and the decoding run in `vauchi-linux-hw`; this shell shows a
//! plain "scanning" dialog with a cancel, no viewfinder. The preview is a
//! PipeWire stream by the ADR-077 amendment and comes with its own record;
//! until then Linux scans blind, as the paste fallback already does.
#[cfg(all(feature = "camera", target_os = "linux"))]
mod inner {
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;
    use std::sync::{Arc, Mutex};

    use gtk4::prelude::*;
    use libadwaita as adw;
    use libadwaita::prelude::*;

    use vauchi_app::i18n;
    use vauchi_app::ui::AppEngine;
    use vauchi_core::{CameraPurpose, Event};
    use vauchi_hardware::{CameraSession, NokhwaCamera};

    use crate::locale::detect_locale;
    use crate::platform::executors::{INBOX_LIMITS, lock, pump, runtime};

    const PREVIEW_ID: &str = "gtk-qr-scan";

    static SESSION: Mutex<Option<CameraSession>> = Mutex::new(None);

    thread_local! {
        static GENERATION: Cell<u64> = const { Cell::new(0) };
    }

    fn session() -> std::sync::MutexGuard<'static, Option<CameraSession>> {
        let mut slot = lock(&SESSION);
        if slot.is_none() {
            *slot = CameraSession::with_backend(
                Arc::new(NokhwaCamera),
                runtime().handle().clone(),
                INBOX_LIMITS,
            )
            .ok();
        }
        slot
    }

    fn pop() -> Option<Event> {
        lock(&SESSION).as_mut().and_then(CameraSession::pop)
    }

    fn end_scan() {
        GENERATION.with(|g| g.set(g.get().wrapping_add(1)));
        if let Some(session) = lock(&SESSION).as_mut() {
            session.stop(PREVIEW_ID);
        }
    }

    /// `QrRequestScan`: scan with the camera until a code decodes; Core
    /// receives it as `QrScanned`.
    pub fn scan_qr(
        container: &gtk4::Box,
        app_engine: &Rc<RefCell<AppEngine>>,
        toast_overlay: &adw::ToastOverlay,
    ) {
        let Some(window) = container
            .root()
            .and_then(|r| r.downcast::<gtk4::Window>().ok())
        else {
            return;
        };
        let generation = GENERATION.with(|g| {
            g.set(g.get().wrapping_add(1));
            g.get()
        });
        let started = session().as_mut().is_some_and(|session| {
            session
                .capture(PREVIEW_ID.into(), CameraPurpose::QrScan)
                .is_ok()
        });
        if !started {
            crate::core_ui::contextual_surface::dispatch_platform_event(
                container,
                app_engine,
                toast_overlay,
                Event::HardwareUnavailable {
                    transport: "Camera".into(),
                },
            );
            return;
        }
        let locale = detect_locale();
        let dialog = adw::MessageDialog::new(
            Some(&window),
            Some(&i18n::get_string(locale, "platform.qr_scan_title")),
            Some(&i18n::get_string(
                locale,
                "platform.qr_scan_instruction_camera",
            )),
        );
        dialog.add_response(
            "cancel",
            &i18n::get_string(locale, "platform.button_cancel"),
        );
        dialog.set_default_response(Some("cancel"));
        dialog.set_close_response("cancel");
        dialog.connect_response(None, move |dlg, _| {
            if GENERATION.with(|g| g.get() == generation) {
                end_scan();
            }
            dlg.close();
        });
        dialog.present();
        let dialog_for_pump = dialog.clone();
        pump(
            container,
            app_engine,
            toast_overlay,
            move || {
                let event = pop();
                if matches!(
                    event,
                    Some(Event::QrScanned { .. } | Event::HardwareError { .. })
                ) {
                    // The scan is over either way; the next request opens anew.
                    GENERATION.with(|g| g.set(g.get().wrapping_add(1)));
                    dialog_for_pump.close();
                }
                event
            },
            move || GENERATION.with(|g| g.get() == generation),
        );
    }
}

#[cfg(all(feature = "camera", target_os = "linux"))]
pub use inner::*;
