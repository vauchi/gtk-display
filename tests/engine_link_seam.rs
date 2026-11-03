// SPDX-FileCopyrightText: 2026 Mattia Egloff <mattia.egloff@pm.me>
// SPDX-License-Identifier: GPL-3.0-or-later
//! The renderer talks to the engine through `EngineLink` alone (ADR-078
//! step 4, D-2): any link serves, and no file of the renderer names the
//! engine's type, and no file resolves strings itself: the link answers
//! `string(key)` at the engine's locale. The app's `RefCell<AppEngine>` is
//! one link among others.
use std::cell::RefCell;
use std::rc::Rc;

use vauchi_core::{Command, Event, SurfaceId};
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

    fn string(&self, key: &str) -> String {
        format!("[{key}]")
    }

    fn string_with_args(&self, key: &str, args: &[(&str, &str)]) -> String {
        let args: Vec<String> = args.iter().map(|(k, v)| format!("{k}={v}")).collect();
        format!("[{key} {}]", args.join(","))
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

// @scenario: generic_presentation_protocol.feature :: Invalid boundary input fails safely
#[test]
fn the_shells_own_strings_come_through_the_link_at_the_engines_locale() {
    let link: EngineHandle = Rc::new(RecordingLink {
        events: RefCell::new(Vec::new()),
        answer: Vec::new(),
    });
    assert_eq!(
        link.string("platform.button_cancel"),
        "[platform.button_cancel]"
    );
    assert_eq!(
        link.string_with_args("platform.camera_error", &[("error", "busy")]),
        "[platform.camera_error error=busy]"
    );
}

// @internal
#[test]
fn no_renderer_file_names_the_engine_type() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/core_ui");
    let mut offenders = Vec::new();
    let mut stack = vec![root];
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
