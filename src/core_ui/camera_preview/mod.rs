// SPDX-FileCopyrightText: 2026 Mattia Egloff <mattia.egloff@pm.me>
// SPDX-License-Identifier: GPL-3.0-or-later

mod frame;
#[cfg(target_os = "linux")]
mod native;
#[cfg(target_os = "linux")]
mod native_widget;
#[cfg(all(feature = "camera-preview", target_os = "linux"))]
mod transport;
mod widgets;
#[cfg(target_os = "linux")]
pub use native::{FrameResolver, FrameResource, NativePreviewReader};
#[cfg(target_os = "linux")]
pub use widgets::set_frame_resolver;

pub use widgets::{picture, set_viewport};

pub use frame::PreviewFrame;

#[cfg(target_os = "linux")]
pub struct PreviewSource {
    pub node_id: u32,
    pub serial: u64,
    pub remote_fd: Option<std::os::fd::OwnedFd>,
}

#[cfg(target_os = "linux")]
pub type SourceResolver = std::rc::Rc<dyn Fn(&str) -> Result<Option<PreviewSource>, String>>;

#[cfg(target_os = "linux")]
pub use widgets::set_source_resolver;

pub fn preview_node_name(preview_id: &str) -> Option<String> {
    if preview_id.is_empty() || preview_id.len() > 128 {
        return None;
    }
    let identity: String = preview_id
        .bytes()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    Some(format!("vauchi.camera.{}.{identity}", std::process::id()))
}

/// A sink may connect only to the exact source prepared by its app.
/// Removal retires the binding; PipeWire must never select a replacement.
pub struct PreviewBinding {
    name: String,
    expected: Option<(u32, u64)>,
    current: Option<(u32, String)>,
    retired: bool,
}

impl PreviewBinding {
    pub fn new(preview_id: &str) -> Option<Self> {
        Some(Self {
            name: preview_node_name(preview_id)?,
            expected: None,
            current: None,
            retired: false,
        })
    }

    pub fn for_source(node_id: u32, serial: u64) -> Option<Self> {
        if node_id == u32::MAX || serial == 0 {
            return None;
        }
        Some(Self {
            name: String::new(),
            expected: Some((node_id, serial)),
            current: None,
            retired: false,
        })
    }

    pub fn observe(&mut self, id: u32, name: &str, media_class: &str, serial: &str) -> bool {
        if self.retired
            || self.current.is_some()
            || id == u32::MAX
            || match self.expected {
                Some((expected_id, expected_serial)) => {
                    id != expected_id || serial.parse::<u64>().ok() != Some(expected_serial)
                }
                None => self.name != name,
            }
            || media_class != "Video/Source"
            || serial.parse::<u64>().ok().is_none_or(|serial| serial == 0)
        {
            return false;
        }
        self.current = Some((id, serial.to_owned()));
        true
    }

    pub fn current(&self) -> Option<(u32, &str)> {
        self.current
            .as_ref()
            .map(|(id, serial)| (*id, serial.as_str()))
    }

    pub fn remove(&mut self, id: u32) {
        if self
            .current
            .as_ref()
            .is_some_and(|(current, _)| *current == id)
        {
            self.current = None;
            self.retired = true;
        }
    }

    pub fn retired(&self) -> bool {
        self.retired
    }
}
