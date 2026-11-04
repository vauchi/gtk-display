// SPDX-FileCopyrightText: 2026 Mattia Egloff <mattia.egloff@pm.me>
// SPDX-License-Identifier: GPL-3.0-or-later
//! The renderer talks to the engine through `EngineLink` alone (ADR-078
//! step 4, D-2): any link serves, and no file of the renderer names the
//! engine's type or asks it for copy: Core sends every word in the
//! commands. The app's `RefCell<AppEngine>` is one link among others.
use std::cell::RefCell;
use std::rc::Rc;

use vauchi_contract::{Command, Event, SurfaceId};
use vauchi_gtk::core_ui::contextual_surface::commands_for_event;
use vauchi_gtk::core_ui::engine_link::{EngineHandle, EngineLink};

struct RecordingLink {
    events: RefCell<Vec<Event>>,
    answer: Vec<Command>,
}

impl EngineLink for RecordingLink {
    fn dispatch(&self, event: Event) -> Vec<Command> {
        self.events.borrow_mut().push(event);
        self.answer.clone()
    }

    fn initial_commands(&self) -> Vec<Command> {
        self.answer.clone()
    }

    fn take_invalidation(&self) -> bool {
        false
    }
}

// @scenario: generic_presentation_protocol.feature :: Invalid boundary input fails safely
#[test]
fn a_shell_event_reaches_whatever_link_the_app_gave_and_its_answer_comes_back() {
    let link = Rc::new(RecordingLink {
        events: RefCell::new(Vec::new()),
        answer: vec![Command::QrRequestScan],
    });
    let handle: EngineHandle = link.clone();
    let commands = commands_for_event(
        handle.as_ref(),
        Event::SurfaceActivated {
            surface_id: SurfaceId::new("contacts").expect("a valid surface id"),
        },
    );
    assert_eq!(commands, vec![Command::QrRequestScan]);
    assert_eq!(
        link.events.borrow().clone(),
        vec![Event::SurfaceActivated {
            surface_id: SurfaceId::new("contacts").expect("a valid surface id")
        }]
    );
}

// @internal
#[test]
fn no_renderer_file_names_the_engine_type() {
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let shell = src.join("shell.rs");
    let mut offenders = Vec::new();
    let shell_text = std::fs::read_to_string(&shell).expect("read src/shell.rs");
    if shell_text.contains("AppEngine") || shell_text.contains("i18n::") {
        offenders.push(shell.display().to_string());
    }
    let mut stack = vec![src.join("core_ui")];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("read src/core_ui") {
            let path = entry.expect("dir entry").path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "rs") && {
                let text = std::fs::read_to_string(&path).expect("read source");
                text.contains("AppEngine")
                    || text.contains("vauchi_app::i18n")
                    || text.contains("i18n::")
            } {
                offenders.push(path.display().to_string());
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "the renderer names the engine type in: {offenders:?}; go through EngineLink"
    );
}

// @internal — Core prepares every word the renderer shows (ADR-066), so
// nothing in it asks the engine link for copy (private#1029 4d.7d).
#[test]
fn no_renderer_file_asks_the_engine_for_copy() {
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut offenders = Vec::new();
    let mut stack = vec![src.join("core_ui"), src.join("platform")];
    let mut files = vec![src.join("shell.rs")];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("read source dir") {
            let path = entry.expect("dir entry").path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "rs") {
                files.push(path);
            }
        }
    }
    for path in files {
        let text = std::fs::read_to_string(&path).expect("read source");
        if text.contains(".string(\"") || text.contains(".string_with_args(") {
            offenders.push(path.display().to_string());
        }
    }
    assert!(
        offenders.is_empty(),
        "the renderer asks the engine for copy in: {offenders:?}; Core sends it"
    );
}
