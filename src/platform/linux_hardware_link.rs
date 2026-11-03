// SPDX-FileCopyrightText: 2026 Mattia Egloff <mattia.egloff@pm.me>
// SPDX-License-Identifier: GPL-3.0-or-later
//! gvauchi's hardware link: linux-hw's `LinuxHardware` behind the
//! renderer's `HardwareLink` (ADR-078 step 3a). The commands linux-hw
//! ports always go to it; a port without a device answers `Unsupported`
//! and reports nothing (owner, 2026-11-02). Everything else stays with the
//! renderer. This wiring moves to linux-app in step 3b.
use std::cell::RefCell;

use vauchi_contract::{Command, Event};
use vauchi_hardware::LinuxHardware;

use crate::core_ui::hardware_link::HardwareLink;

pub struct LinuxHardwareLink(RefCell<LinuxHardware>);

impl LinuxHardwareLink {
    pub fn new(hardware: LinuxHardware) -> Self {
        Self(RefCell::new(hardware))
    }

    /// The capabilities to report to Core once, at start.
    pub fn report(&self) -> Event {
        self.0.borrow().report()
    }
}

/// The commands linux-hw has a port for (`HardwareShell::execute`).
pub fn ported(command: &Command) -> bool {
    matches!(
        command,
        Command::BleStartScanning { .. }
            | Command::BleStopScanning
            | Command::BleStartAdvertising { .. }
            | Command::BleStopAdvertising
            | Command::BleConnect { .. }
            | Command::BleWriteCharacteristic { .. }
            | Command::BleReadCharacteristic { .. }
            | Command::BleDisconnect { .. }
            | Command::NfcActivate { .. }
            | Command::NfcSendApdu { .. }
            | Command::NfcDeactivate
            | Command::AudioEmitChallenge { .. }
            | Command::AudioListenForResponse { .. }
            | Command::AudioStop
            | Command::CameraCapture { .. }
            | Command::CameraStop { .. }
    )
}

impl HardwareLink for LinuxHardwareLink {
    fn execute(&self, command: &Command) -> bool {
        if !ported(command) {
            return false;
        }
        // A port without a device answers Unsupported: report nothing.
        let _ = self.0.borrow_mut().execute(command.clone());
        true
    }

    fn pop(&self) -> Option<Event> {
        self.0.borrow_mut().pop()
    }
}
