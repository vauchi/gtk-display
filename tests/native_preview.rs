// SPDX-FileCopyrightText: 2026 Mattia Egloff <mattia.egloff@pm.me>
// SPDX-License-Identifier: GPL-3.0-or-later

#![cfg(target_os = "linux")]
use std::{fs::File, io::ErrorKind, os::unix::fs::FileExt};
use vauchi_gtk::core_ui::camera_preview::NativePreviewReader;

fn resource() -> File {
    let file = tempfile::tempfile().unwrap();
    file.set_len(32 + 1920 * 1080 * 4).unwrap();
    let mut header = Vec::from(*b"VFRAME01");
    for word in [2u32, 1, 8, 2] {
        header.extend(word.to_le_bytes());
    }
    header.extend(1u64.to_le_bytes());
    file.write_all_at(&header, 0).unwrap();
    file.write_all_at(&[1, 2, 3, 255, 4, 5, 6, 255], 32)
        .unwrap();
    file
}

// @internal — consume a native resource prepared by the app, without capture libraries.
#[test]
fn native_preview_owns_pixels_skips_duplicates_and_clears_on_retirement() {
    let file = resource();
    let mut reader = NativePreviewReader::new(file.try_clone().unwrap().into()).unwrap();
    let frame = reader.poll().unwrap().unwrap();
    assert_eq!((frame.width(), frame.height()), (2, 1));
    assert_eq!(frame.rgba(), &[1, 2, 3, 255, 4, 5, 6, 255]);
    assert_eq!(reader.poll().unwrap(), None);
    file.write_all_at(&[0; 24], 8).unwrap();
    assert_eq!(reader.poll().unwrap(), None);
    assert!(reader.closed());
    file.write_all_at(&[9; 8], 32).unwrap();
    assert_eq!(frame.rgba(), &[1, 2, 3, 255, 4, 5, 6, 255]);
}

// @internal
#[test]
fn malformed_native_resources_fail_before_rendering() {
    assert!(NativePreviewReader::new(tempfile::tempfile().unwrap().into()).is_err());
    for (offset, bytes) in [
        (0, vec![0; 8]),
        (8, u32::MAX.to_le_bytes().to_vec()),
        (16, 0u32.to_le_bytes().to_vec()),
        (20, 9u32.to_le_bytes().to_vec()),
        (24, 0u64.to_le_bytes().to_vec()),
    ] {
        let file = resource();
        let mut reader = NativePreviewReader::new(file.try_clone().unwrap().into()).unwrap();
        file.write_all_at(&bytes, offset).unwrap();
        assert_eq!(reader.poll().unwrap_err().kind(), ErrorKind::InvalidData);
    }
}
