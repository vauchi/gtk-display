// SPDX-FileCopyrightText: 2026 Mattia Egloff <mattia.egloff@pm.me>
// SPDX-License-Identifier: GPL-3.0-or-later
//! The seam between this display shell and the hardware shell (ADR-078
//! step 3a, ADR-077): the app installs a [`HardwareLink`] (linux-hw's
//! `LinuxHardware` in gvauchi), every command Core targets at hardware goes
//! to it first, and its events reach Core on the main loop. A hardware
//! command the link has no port for (QR scan, wakeups, notifications still
//! run in this shell) stays with the renderer's own arms.
use std::cell::RefCell;
use std::rc::Rc;

use gtk4::glib;
use libadwaita as adw;
use vauchi_contract::{Command, CommandTarget, Event};

use crate::core_ui::engine_link::EngineHandle;

/// Starts the app's hardware shell on the main thread, once the UI exists:
/// the link and the capabilities report Core gets first, or `None` when
/// there is no hardware to drive (every command then stays here).
pub type StartHardware = Box<dyn FnOnce() -> Option<StartedHardware>>;

pub struct StartedHardware {
    pub link: Rc<dyn HardwareLink>,
    pub capabilities: Option<Event>,
}

pub trait HardwareLink {
    /// Run the command on a hardware port; `false` when no port takes it.
    fn execute(&self, command: &Command) -> bool;

    /// The next event a port reported, if any.
    fn pop(&self) -> Option<Event>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Routed {
    ToHardware,
    ToDisplay,
}

/// Where a command runs: on the link when it targets hardware and a port
/// takes it, with the renderer otherwise.
pub fn route(command: &Command, link: Option<&dyn HardwareLink>) -> Routed {
    match link {
        Some(link) if command.target() == CommandTarget::Hardware && link.execute(command) => {
            Routed::ToHardware
        }
        _ => Routed::ToDisplay,
    }
}

thread_local! {
    static LINK: RefCell<Option<Rc<dyn HardwareLink>>> = const { RefCell::new(None) };
}

/// Install the app's hardware link and drain its events into the engine
/// every 50 ms on the main loop.
pub fn install(
    link: Rc<dyn HardwareLink>,
    container: &gtk4::Box,
    engine: &EngineHandle,
    toast_overlay: &adw::ToastOverlay,
) {
    LINK.with(|slot| *slot.borrow_mut() = Some(link));
    let container = container.clone();
    let engine = engine.clone();
    let toast_overlay = toast_overlay.clone();
    // The pump reads the slot on each tick instead of holding the link, so
    // `uninstall` really drops it; the pump stops with it.
    glib::timeout_add_local(std::time::Duration::from_millis(50), move || {
        let Some(link) = LINK.with(|slot| slot.borrow().clone()) else {
            return glib::ControlFlow::Break;
        };
        while let Some(event) = link.pop() {
            crate::core_ui::contextual_surface::dispatch_platform_event(
                &container,
                &engine,
                &toast_overlay,
                event,
            );
        }
        glib::ControlFlow::Continue
    });
}

/// Drop the installed link, and with it the hardware shell's runtime, while
/// the main loop still runs.
pub fn uninstall() {
    let link = LINK.with(|slot| slot.borrow_mut().take());
    drop(link);
}

/// [`route`] against the installed link.
pub(crate) fn route_installed(command: &Command) -> Routed {
    let link = LINK.with(|slot| slot.borrow().clone());
    route(command, link.as_deref())
}
