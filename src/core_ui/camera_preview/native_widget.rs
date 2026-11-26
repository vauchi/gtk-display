// SPDX-FileCopyrightText: 2026 Mattia Egloff <mattia.egloff@pm.me>
// SPDX-License-Identifier: GPL-3.0-or-later

use super::{FrameResource, NativePreviewReader};
use gtk4::{glib, prelude::*};

pub(super) fn attach(picture: &gtk4::Picture) {
    let weak = picture.downgrade();
    let mut reader = None::<NativePreviewReader>;
    let mut identity = None;
    let mut generation = 0;
    glib::timeout_add_local(std::time::Duration::from_millis(30), move || {
        let Some(picture) = weak.upgrade() else {
            return glib::ControlFlow::Break;
        };
        let Some(resolve) = super::widgets::frame_resolver() else {
            if identity.take().is_some() {
                picture.set_paintable(None::<&gtk4::gdk::Paintable>);
            }
            reader = None;
            generation = 0;
            return glib::ControlFlow::Continue;
        };
        let id = super::widgets::viewport();
        if !picture.is_mapped() || id != identity {
            if identity.is_some() {
                picture.set_paintable(None::<&gtk4::gdk::Paintable>);
            }
            reader = None;
            generation = 0;
            identity = id.clone();
        }
        if picture.is_mapped()
            && let Some(id) = id
        {
            match resolve(&id, generation) {
                Ok(FrameResource::New {
                    generation: next,
                    fd,
                }) if next > 0 && next != generation => {
                    reader = NativePreviewReader::new(fd).ok();
                    generation = next;
                    picture.set_paintable(None::<&gtk4::gdk::Paintable>);
                }
                Ok(FrameResource::Unchanged) => {}
                _ => {
                    reader = None;
                    generation = 0;
                    picture.set_paintable(None::<&gtk4::gdk::Paintable>);
                }
            }
            if let Some(source) = reader.as_mut() {
                match source.poll() {
                    Ok(Some(frame)) => picture.set_paintable(Some(&frame.texture())),
                    Ok(None) if !source.closed() => {}
                    _ => {
                        picture.set_paintable(None::<&gtk4::gdk::Paintable>);
                        reader = None;
                    }
                }
            }
        }
        glib::ControlFlow::Continue
    });
}
