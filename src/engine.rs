// SPDX-FileCopyrightText: 2026 Mattia Egloff <mattia.egloff@pm.me>
// SPDX-License-Identifier: GPL-3.0-or-later
//! The app's engine as the renderer's [`EngineLink`]: wiring, not
//! renderer, so it moves to `linux-app` with the rest of the wiring
//! (ADR-078 step 4d). A newtype, not `RefCell<AppEngine>` itself: in
//! linux-app neither the trait nor `AppEngine` is local (orphan rule).
use std::cell::RefCell;
use std::sync::mpsc::Receiver;

use vauchi_app::ui::AppEngine;
use vauchi_contract::{Command, Event};

use crate::core_ui::engine_link::EngineLink;

pub struct CoreEngine {
    engine: RefCell<AppEngine>,
    invalidations: Receiver<()>,
}

impl CoreEngine {
    pub fn new(engine: AppEngine) -> Self {
        let invalidations = engine.subscribe_presentation_invalidations();
        Self {
            engine: RefCell::new(engine),
            invalidations,
        }
    }
}

trait EngineLocale {
    fn locale(&self) -> vauchi_app::i18n::Locale;
}

impl EngineLocale for CoreEngine {
    /// The engine's render-context locale tag, or the shell's detected
    /// locale while the engine has none yet.
    fn locale(&self) -> vauchi_app::i18n::Locale {
        self.engine
            .borrow()
            .render_context()
            .locale
            .as_deref()
            .and_then(vauchi_app::i18n::translated_locale)
            .unwrap_or_else(crate::locale::detect_locale)
    }
}

impl EngineLink for CoreEngine {
    fn dispatch(&self, event: Event) -> Vec<Command> {
        let outcome = self.engine.borrow_mut().dispatch(event);
        outcome.unwrap_or_else(|error| {
            // The kind only: a rejection's Display text can echo user input.
            eprintln!("[CoreUI] Failed: dispatch rejected: {}", error.kind());
            self.engine.borrow().reject_dispatch(&error)
        })
    }

    fn initial_commands(&self) -> Vec<Command> {
        self.engine
            .borrow_mut()
            .initial_commands()
            .unwrap_or_default()
    }

    fn string(&self, key: &str) -> String {
        vauchi_app::i18n::get_string(self.locale(), key)
    }

    fn string_with_args(&self, key: &str, args: &[(&str, &str)]) -> String {
        vauchi_app::i18n::get_string_with_args(self.locale(), key, args)
    }

    fn take_invalidation(&self) -> bool {
        self.invalidations.try_recv().is_ok()
    }
}
