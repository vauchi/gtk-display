// SPDX-FileCopyrightText: 2026 Mattia Egloff <mattia.egloff@pm.me>
// SPDX-License-Identifier: GPL-3.0-or-later

use super::PreviewFrame;
use std::{
    fs::{File, TryLockError},
    io,
    os::{fd::OwnedFd, unix::fs::FileExt},
    rc::Rc,
};

pub enum FrameResource {
    Absent,
    Unchanged,
    New { generation: u64, fd: OwnedFd },
}
pub type FrameResolver = Rc<dyn Fn(&str, u64) -> Result<FrameResource, String>>;

/// Reads a prepared RGBA resource. It never discovers or opens a camera.
pub struct NativePreviewReader {
    file: File,
    generation: u64,
    closed: bool,
}

impl NativePreviewReader {
    pub fn new(fd: OwnedFd) -> io::Result<Self> {
        let file = File::from(fd);
        validate_file(&file)?;
        Ok(Self {
            file,
            generation: 0,
            closed: false,
        })
    }

    pub fn closed(&self) -> bool {
        self.closed
    }

    pub fn poll(&mut self) -> io::Result<Option<PreviewFrame>> {
        if self.closed {
            return Ok(None);
        }
        match self.file.try_lock_shared() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => return Ok(None),
            Err(TryLockError::Error(error)) => return Err(error),
        }
        let result = self.read();
        let unlocked = self.file.unlock();
        if result.is_err() {
            self.closed = true;
        }
        unlocked?;
        result
    }

    fn read(&mut self) -> io::Result<Option<PreviewFrame>> {
        validate_file(&self.file)?;
        let mut header = [0; 32];
        self.file.read_exact_at(&mut header, 0)?;
        if &header[..8] != b"VFRAME01" {
            return Err(invalid());
        }
        let word = |offset| u32::from_le_bytes(header[offset..offset + 4].try_into().unwrap());
        let (width, height, bytes, state) = (word(8), word(12), word(16), word(20));
        let generation = u64::from_le_bytes(header[24..].try_into().unwrap());
        match state {
            0 | 1 if width == 0 && height == 0 && bytes == 0 => {
                self.closed = state == 0;
                return Ok(None);
            }
            2 if width > 0
                && width <= 1920
                && height > 0
                && height <= 1080
                && bytes == width * height * 4
                && generation > 0 => {}
            _ => return Err(invalid()),
        }
        if generation == self.generation {
            return Ok(None);
        }
        let mut pixels = vec![0; bytes as usize];
        self.file.read_exact_at(&mut pixels, 32)?;
        let frame = PreviewFrame::from_rgba(width, height, width as usize * 4, &pixels)
            .ok_or_else(invalid)?;
        self.generation = generation;
        Ok(Some(frame))
    }
}

fn validate_file(file: &File) -> io::Result<()> {
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() != 32 + 1920 * 1080 * 4 {
        return Err(invalid());
    }
    Ok(())
}
fn invalid() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "invalid native preview resource",
    )
}
