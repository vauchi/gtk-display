// SPDX-FileCopyrightText: 2026 Mattia Egloff <mattia.egloff@pm.me>
// SPDX-License-Identifier: GPL-3.0-or-later

use gtk4::Picture;
use gtk4::prelude::*;
use std::cell::RefCell;

thread_local! {
    static VIEWPORT: RefCell<Option<String>> = const { RefCell::new(None) };
}

pub fn set_viewport(preview_id: Option<&str>) {
    VIEWPORT.with(|current| {
        *current.borrow_mut() = preview_id
            .filter(|id| super::preview_node_name(id).is_some())
            .map(str::to_owned);
    });
}

pub fn picture(accessible_label: &str) -> Picture {
    let picture = Picture::new();
    picture.set_can_shrink(true);
    picture.set_content_fit(gtk4::ContentFit::Contain);
    picture.set_accessible_role(gtk4::AccessibleRole::Img);
    picture.set_widget_name("camera-preview");
    crate::core_ui::accessibility::apply_label(&picture, accessible_label);
    #[cfg(all(feature = "camera-preview", target_os = "linux"))]
    attach(&picture);
    picture
}

#[cfg(all(feature = "camera-preview", target_os = "linux"))]
fn attach(picture: &Picture) {
    use gtk4::glib;
    let weak = picture.downgrade();
    let mut source = None;
    let mut attempted = None;
    glib::timeout_add_local(std::time::Duration::from_millis(30), move || {
        let Some(picture) = weak.upgrade() else {
            return glib::ControlFlow::Break;
        };
        if picture.root().is_none() {
            source = None;
            attempted = None;
            picture.set_paintable(None::<&gtk4::gdk::Paintable>);
            return glib::ControlFlow::Continue;
        }
        let preview_id = VIEWPORT.with(|id| id.borrow().clone());
        if attempted != preview_id {
            attempted = preview_id.clone();
            picture.set_paintable(None::<&gtk4::gdk::Paintable>);
            source = preview_id
                .as_deref()
                .and_then(super::transport::PreviewStream::new);
        }
        if let Some(client) = source.as_mut() {
            if let Some(frame) = client.poll() {
                picture.set_paintable(Some(&frame.texture()));
            }
            if client.closed() {
                picture.set_paintable(None::<&gtk4::gdk::Paintable>);
                source = None;
            }
        }
        glib::ControlFlow::Continue
    });
}
