// SPDX-FileCopyrightText: 2026 Mattia Egloff <mattia.egloff@pm.me>
// SPDX-License-Identifier: GPL-3.0-or-later

#![cfg(target_os = "linux")]
use gtk4::prelude::*;
use std::{
    cell::RefCell,
    fs::File,
    os::unix::fs::FileExt,
    rc::Rc,
    time::{Duration, Instant},
};
use vauchi_gtk::core_ui::camera_preview::{
    FrameResource, picture, set_frame_resolver, set_viewport,
};

fn resource(width: u32, height: u32) -> File {
    let file = tempfile::tempfile().unwrap();
    file.set_len(32 + 1920 * 1080 * 4).unwrap();
    let mut header = Vec::from(*b"VFRAME01");
    for word in [width, height, width * height * 4, 2] {
        header.extend(word.to_le_bytes());
    }
    header.extend(1u64.to_le_bytes());
    file.write_all_at(&header, 0).unwrap();
    file.write_all_at(&vec![255; width as usize * height as usize * 4], 32)
        .unwrap();
    file
}

fn until(condition: impl Fn() -> bool) {
    let started = Instant::now();
    while !condition() {
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "native preview did not update"
        );
        gtk4::glib::MainContext::default().iteration(false);
        std::thread::yield_now();
    }
}

// @internal — real GTK texture, same-ID replacement, closure and unmapping.
#[test]
fn native_camera_resource_replacement_and_retirement_reach_the_widget() {
    if std::env::var_os("VAUCHI_NATIVE_WIDGET_CHILD").is_none() {
        let output = std::process::Command::new("xvfb-run")
            .arg("-a")
            .arg(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "native_camera_resource_replacement_and_retirement_reach_the_widget",
                "--nocapture",
            ])
            .env("VAUCHI_NATIVE_WIDGET_CHILD", "1")
            .env("GDK_BACKEND", "x11")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
        return;
    }
    gtk4::init().unwrap();
    let state = Rc::new(RefCell::new((1, resource(2, 1))));
    let resolved = state.clone();
    set_frame_resolver(Some(Rc::new(move |id, previous| {
        assert_eq!(id, "scan");
        let (generation, file) = &*resolved.borrow();
        if previous == *generation {
            return Ok(FrameResource::Unchanged);
        }
        use std::os::fd::AsRawFd;
        let fd = File::open(format!("/proc/self/fd/{}", file.as_raw_fd()))
            .unwrap()
            .into();
        Ok(FrameResource::New {
            generation: *generation,
            fd,
        })
    })));
    set_viewport(Some("scan"));
    let preview = picture("Scan QR code");
    let window = gtk4::Window::new();
    window.set_child(Some(&preview));
    window.present();
    until(|| {
        preview
            .paintable()
            .is_some_and(|image| image.intrinsic_width() == 2)
    });
    assert_eq!(preview.paintable().unwrap().intrinsic_height(), 1);
    *state.borrow_mut() = (2, resource(3, 2));
    until(|| {
        preview
            .paintable()
            .is_some_and(|image| image.intrinsic_width() == 3)
    });
    assert_eq!(preview.paintable().unwrap().intrinsic_height(), 2);
    state.borrow().1.write_all_at(&[0; 24], 8).unwrap();
    until(|| preview.paintable().is_none());
    assert!(preview.paintable().is_none());
    *state.borrow_mut() = (3, resource(4, 3));
    until(|| {
        preview
            .paintable()
            .is_some_and(|image| image.intrinsic_width() == 4)
    });
    window.set_visible(false);
    until(|| preview.paintable().is_none());
    assert!(preview.paintable().is_none());
    set_viewport(None);
    set_frame_resolver(None);
    window.close();
}
