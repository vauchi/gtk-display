// SPDX-FileCopyrightText: 2026 Mattia Egloff <mattia.egloff@pm.me>
// SPDX-License-Identifier: GPL-3.0-or-later

//! Native file-picker effects for generic Core commands.

use gtk4::Box as GtkBox;
use gtk4::prelude::*;
use libadwaita as adw;

use vauchi_core::{Event, ExportFileSpec};

use crate::core_ui::engine_link::EngineHandle;

use crate::core_ui::contextual_surface::dispatch_platform_event;

pub(super) fn open_file_picker(
    container: &GtkBox,
    app_engine: &EngineHandle,
    toast_overlay: &adw::ToastOverlay,
    accepted_mime_types: &[String],
) {
    let Some(window) = parent_window(container) else {
        return;
    };
    let filter = gtk4::FileFilter::new();
    for mime_type in accepted_mime_types {
        filter.add_mime_type(mime_type);
    }
    let filters = gtk4::gio::ListStore::new::<gtk4::FileFilter>();
    filters.append(&filter);
    let dialog = gtk4::FileDialog::builder().filters(&filters).build();
    let app_engine = app_engine.clone();
    let container = container.clone();
    let toast_overlay = toast_overlay.clone();

    dialog.open(
        Some(&window),
        None::<&gtk4::gio::Cancellable>,
        move |result| match result {
            Ok(file) => {
                let filename = file
                    .basename()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let Some(path) = file.path() else {
                    report_read_error(&container, &app_engine, &toast_overlay);
                    return;
                };
                match std::fs::read(path) {
                    Ok(bytes) => dispatch_platform_event(
                        &container,
                        &app_engine,
                        &toast_overlay,
                        Event::FilePickedFromUser { bytes, filename },
                    ),
                    Err(_) => report_read_error(&container, &app_engine, &toast_overlay),
                }
            }
            Err(_) => dispatch_platform_event(
                &container,
                &app_engine,
                &toast_overlay,
                Event::FilePickCancelledByUser,
            ),
        },
    );
}

pub(super) fn open_image_picker(
    container: &GtkBox,
    app_engine: &EngineHandle,
    toast_overlay: &adw::ToastOverlay,
) {
    let Some(window) = parent_window(container) else {
        return;
    };
    let filter = gtk4::FileFilter::new();
    for mime_type in ["image/png", "image/jpeg", "image/webp", "image/bmp"] {
        filter.add_mime_type(mime_type);
    }
    let filters = gtk4::gio::ListStore::new::<gtk4::FileFilter>();
    filters.append(&filter);
    let dialog = gtk4::FileDialog::builder().filters(&filters).build();
    let app_engine = app_engine.clone();
    let container = container.clone();
    let toast_overlay = toast_overlay.clone();

    dialog.open(
        Some(&window),
        None::<&gtk4::gio::Cancellable>,
        move |result| match result {
            Ok(file) => match file.path().and_then(|path| std::fs::read(path).ok()) {
                Some(data) => dispatch_platform_event(
                    &container,
                    &app_engine,
                    &toast_overlay,
                    Event::ImageReceived { data },
                ),
                None => report_read_error(&container, &app_engine, &toast_overlay),
            },
            Err(_) => dispatch_platform_event(
                &container,
                &app_engine,
                &toast_overlay,
                Event::ImagePickCancelled,
            ),
        },
    );
}

pub(super) fn open_export(
    container: &GtkBox,
    app_engine: &EngineHandle,
    toast_overlay: &adw::ToastOverlay,
    export: ExportFileSpec,
) {
    let Some(window) = parent_window(container) else {
        return;
    };
    let dialog = gtk4::FileDialog::new();
    dialog.set_initial_name(Some(&export.suggested_name));
    let app_engine = app_engine.clone();
    let container = container.clone();
    let toast_overlay = toast_overlay.clone();

    dialog.save(
        Some(&window),
        None::<&gtk4::gio::Cancellable>,
        move |result| {
            let write_result = result
                .ok()
                .and_then(|file| file.path())
                .map(|path| std::fs::write(path, &export.data));
            if !matches!(write_result, Some(Ok(()))) {
                dispatch_platform_event(
                    &container,
                    &app_engine,
                    &toast_overlay,
                    Event::HardwareError {
                        transport: "file_export".into(),
                        error: "Failed to save exported file".into(),
                    },
                );
            }
        },
    );
}

fn parent_window(container: &GtkBox) -> Option<gtk4::Window> {
    container
        .root()
        .and_then(|root| root.downcast::<gtk4::Window>().ok())
}

fn report_read_error(
    container: &GtkBox,
    app_engine: &EngineHandle,
    toast_overlay: &adw::ToastOverlay,
) {
    dispatch_platform_event(
        container,
        app_engine,
        toast_overlay,
        Event::HardwareError {
            transport: "file_picker".into(),
            error: "Failed to read selected file".into(),
        },
    );
}
