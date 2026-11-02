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
use vauchi_core::{Command, Event};

/// What the shell asks of the engine, and nothing more.
pub trait EngineLink {
    /// Core's answer to one event: the batch to apply. A rejected event
    /// yields the alert batch Core prepares for it, so the shell applies
    /// something either way.
    fn dispatch(&self, event: Event) -> Vec<Command>;

    /// The batch that boots the shell.
    fn initial_commands(&self) -> Vec<Command>;
}

/// The handle the app gives the shell; cloned into every widget closure.
pub type EngineHandle = Rc<dyn EngineLink>;

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
}
