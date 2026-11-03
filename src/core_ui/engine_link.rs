// SPDX-FileCopyrightText: 2026 Mattia Egloff <mattia.egloff@pm.me>
// SPDX-License-Identifier: GPL-3.0-or-later
//! The one seam between this display shell and the engine (ADR-078 step
//! 4): widgets emit contract `Event`s and apply the `Command`s that come
//! back; they never name the engine's type. The app owns the engine and
//! hands the shell an [`EngineHandle`]; `RefCell<AppEngine>` implements
//! the link here until the wiring moves to `linux-app`.
use std::cell::RefCell;
use std::rc::Rc;

use vauchi_app::ui::AppEngine;
use vauchi_contract::{Command, Event};

/// What the shell asks of the engine, and nothing more.
pub trait EngineLink {
    /// Core's answer to one event: the batch to apply. A rejected event
    /// yields the alert batch Core prepares for it, so the shell applies
    /// something either way.
    fn dispatch(&self, event: Event) -> Vec<Command>;

    /// The batch that boots the shell.
    fn initial_commands(&self) -> Vec<Command>;

    /// The shell's own chrome text (a cancel button, a placeholder) by
    /// key, at the engine's locale: label text comes from Core's locale
    /// store, never from the shell.
    fn string(&self, key: &str) -> String;

    fn string_with_args(&self, key: &str, args: &[(&str, &str)]) -> String;
}

/// The handle the app gives the shell; cloned into every widget closure.
pub type EngineHandle = Rc<dyn EngineLink>;

trait EngineLocale {
    fn locale(&self) -> vauchi_app::i18n::Locale;
}

impl EngineLocale for RefCell<AppEngine> {
    /// The engine's render-context locale tag, or the shell's detected
    /// locale while the engine has none yet.
    fn locale(&self) -> vauchi_app::i18n::Locale {
        self.borrow()
            .render_context()
            .locale
            .as_deref()
            .and_then(vauchi_app::i18n::translated_locale)
            .unwrap_or_else(crate::locale::detect_locale)
    }
}

impl EngineLink for RefCell<AppEngine> {
    fn dispatch(&self, event: Event) -> Vec<Command> {
        let outcome = self.borrow_mut().dispatch(event);
        outcome.unwrap_or_else(|error| {
            // The kind only: a rejection's Display text can echo user input.
            eprintln!("[CoreUI] Failed: dispatch rejected: {}", error.kind());
            self.borrow().reject_dispatch(&error)
        })
    }

    fn initial_commands(&self) -> Vec<Command> {
        self.borrow_mut().initial_commands().unwrap_or_default()
    }

    fn string(&self, key: &str) -> String {
        vauchi_app::i18n::get_string(self.locale(), key)
    }

    fn string_with_args(&self, key: &str, args: &[(&str, &str)]) -> String {
        vauchi_app::i18n::get_string_with_args(self.locale(), key, args)
    }
}
