// SPDX-FileCopyrightText: 2026 Mattia Egloff <mattia.egloff@pm.me>
// SPDX-License-Identifier: GPL-3.0-or-later

mod common;

use gtk4::prelude::*;
use std::{cell::RefCell, rc::Rc};
use vauchi_contract::*;
use vauchi_gtk::core_ui::{
    engine_link::{EngineHandle, EngineLink},
    generic_surface,
};

struct Engine;
impl EngineLink for Engine {
    fn dispatch(&self, _: Event) -> Vec<Command> {
        Vec::new()
    }
    fn initial_commands(&self) -> Vec<Command> {
        Vec::new()
    }
    fn take_invalidation(&self) -> bool {
        false
    }
}

fn descendants(widget: &gtk4::Widget) -> Vec<gtk4::Widget> {
    let mut result = Vec::new();
    let mut child = widget.first_child();
    while let Some(widget) = child {
        child = widget.next_sibling();
        result.extend(descendants(&widget));
        result.push(widget);
    }
    result
}

// @scenario: contact_exchange.feature :: Successful QR code exchange with proximity
#[test]
fn qr_capture_renders_a_preview_and_preserves_manual_input_events() {
    if std::env::var_os("VAUCHI_CAMERA_WIDGET_TEST_CHILD").is_none() {
        let output = std::process::Command::new("xvfb-run")
            .arg("-a")
            .arg(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "qr_capture_renders_a_preview_and_preserves_manual_input_events",
                "--nocapture",
            ])
            .env("VAUCHI_CAMERA_WIDGET_TEST_CHILD", "1")
            .env("GDK_BACKEND", "x11")
            .output()
            .expect("Xvfb is required for the GTK widget test");
        assert_eq!(
            output.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("test result: ok. 1 passed"));
        return;
    }
    gtk4::init().unwrap();
    let container = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    let surface = SurfaceSpec {
        surface_id: SurfaceId::new("capture").unwrap(),
        revision: 1,
        title: "Scan".into(),
        subtitle: None,
        accessibility_label: "Scan".into(),
        layout: SurfaceLayout::Scroll,
        tokens: common::presentation_tokens(),
        nodes: vec![PresentationNode::Qr {
            id: BindingId::new("qr-input").unwrap(),
            payloads: Vec::new(),
            purpose: PresentationQrPurpose::Capture,
            label: None,
            placement: None,
            error_correction: None,
            size: None,
            accessibility: AccessibilitySpec::label("Scan a contact QR"),
            capture_placeholder: Some("Paste a code".into()),
            capture_submit_label: Some("Use code".into()),
        }],
    };
    let events = Rc::new(RefCell::new(Vec::new()));
    let observed = events.clone();
    let on_event: generic_surface::OnEvent =
        Rc::new(move |event| observed.borrow_mut().push(event));
    let engine: EngineHandle = Rc::new(Engine);
    generic_surface::render(&container, &surface, &on_event, &engine);
    let widgets = descendants(container.upcast_ref());
    let preview = widgets
        .iter()
        .find_map(|widget| widget.clone().downcast::<gtk4::Picture>().ok())
        .expect("QR capture has a native preview area");
    assert_eq!(preview.accessible_role(), gtk4::AccessibleRole::Img);
    let entry = widgets
        .iter()
        .find_map(|widget| widget.clone().downcast::<gtk4::Entry>().ok())
        .unwrap();
    assert_eq!(entry.placeholder_text().as_deref(), Some("Paste a code"));
    entry.set_text("vauchi://exchange/manual");
    let button = widgets
        .iter()
        .find_map(|widget| widget.clone().downcast::<gtk4::Button>().ok())
        .unwrap();
    button.emit_clicked();
    assert_eq!(
        events.borrow().as_slice(),
        &[
            Event::SurfaceActivated {
                surface_id: surface.surface_id.clone()
            },
            Event::ValueChanged {
                surface_id: surface.surface_id,
                binding_id: BindingId::new("qr-input").unwrap(),
                value: InputValue::Text("vauchi://exchange/manual".into())
            },
        ]
    );
}
