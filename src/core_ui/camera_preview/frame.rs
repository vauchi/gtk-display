// SPDX-FileCopyrightText: 2026 Mattia Egloff <mattia.egloff@pm.me>
// SPDX-License-Identifier: GPL-3.0-or-later

use gtk4::{gdk, glib};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreviewFrame {
    width: u32,
    height: u32,
    rgba: glib::Bytes,
}

impl PreviewFrame {
    pub fn from_rgba(width: u32, height: u32, stride: usize, bytes: &[u8]) -> Option<Self> {
        if width == 0 || height == 0 || width > 1920 || height > 1080 {
            return None;
        }
        let row = width as usize * 4;
        if stride < row || stride.checked_mul(height as usize - 1)?.checked_add(row)? > bytes.len()
        {
            return None;
        }
        let mut rgba = Vec::with_capacity(row * height as usize);
        for y in 0..height as usize {
            rgba.extend_from_slice(bytes.get(y * stride..y * stride + row)?);
        }
        Some(Self {
            width,
            height,
            rgba: glib::Bytes::from_owned(rgba),
        })
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn rgba(&self) -> &[u8] {
        self.rgba.as_ref()
    }

    pub fn texture(&self) -> gdk::MemoryTexture {
        gdk::MemoryTexture::new(
            self.width as i32,
            self.height as i32,
            gdk::MemoryFormat::R8g8b8a8,
            &self.rgba,
            self.width as usize * 4,
        )
    }
}
