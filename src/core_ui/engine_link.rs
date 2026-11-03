// SPDX-FileCopyrightText: 2026 Mattia Egloff <mattia.egloff@pm.me>
// SPDX-License-Identifier: GPL-3.0-or-later
//! The one seam between this display shell and the engine (ADR-078 step
//! 4): widgets emit contract `Event`s and apply the `Command`s that come
//! back; they never name the engine's type. The app owns the engine and
//! hands the shell an [`EngineHandle`]; the app's engine implements the
//! link in [`crate::engine`], outside the renderer.
use std::rc::Rc;

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

    /// Whether Core invalidated the presentation since the last call; the
    /// shell polls this and repaints the current surface when it is true.
    fn take_invalidation(&self) -> bool;
}

/// The handle the app gives the shell; cloned into every widget closure.
pub type EngineHandle = Rc<dyn EngineLink>;
