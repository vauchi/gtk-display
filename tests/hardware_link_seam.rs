// SPDX-FileCopyrightText: 2026 Mattia Egloff <mattia.egloff@pm.me>
// SPDX-License-Identifier: GPL-3.0-or-later
//! Hardware commands reach the hardware link the app installed, never the
//! renderer's own arms (ADR-078 step 3a, ADR-077: the display shell runs
//! no hardware). Display commands stay with the renderer.
use std::cell::RefCell;

use vauchi_core::{Command, Event};
use vauchi_gtk::core_ui::hardware_link::{HardwareLink, Routed, route};

#[derive(Default)]
struct RecordingHardware {
    executed: RefCell<Vec<Command>>,
    events: RefCell<Vec<Event>>,
}

impl HardwareLink for RecordingHardware {
    fn execute(&self, command: &Command) {
        self.executed.borrow_mut().push(command.clone());
    }

    fn pop(&self) -> Option<Event> {
        self.events.borrow_mut().pop()
    }
}

// @scenario: contact_exchange.feature :: NFC active exchange between two phones
#[test]
fn a_hardware_command_goes_to_the_installed_link() {
    let hardware = RecordingHardware::default();
    let command = Command::NfcDeactivate;
    assert_eq!(route(&command, Some(&hardware)), Routed::ToHardware);
    assert_eq!(
        hardware.executed.borrow().clone(),
        vec![Command::NfcDeactivate]
    );
}

// @internal
#[test]
fn a_display_command_stays_with_the_renderer() {
    let hardware = RecordingHardware::default();
    let command = Command::QrRequestScan;
    assert_eq!(route(&command, Some(&hardware)), Routed::ToDisplay);
    assert!(hardware.executed.borrow().is_empty());
}

// @internal
#[test]
fn without_a_link_every_command_stays_with_the_renderer() {
    assert_eq!(route(&Command::NfcDeactivate, None), Routed::ToDisplay);
}

// @internal
#[test]
fn the_links_events_drain_in_order() {
    let hardware = RecordingHardware::default();
    hardware.events.borrow_mut().extend([
        Event::NfcFailed {
            reason: "second".into(),
        },
        Event::NfcFailed {
            reason: "first".into(),
        },
    ]);
    let drained: Vec<Event> = std::iter::from_fn(|| hardware.pop()).collect();
    assert_eq!(drained.len(), 2);
}
