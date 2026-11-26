// SPDX-FileCopyrightText: 2026 Mattia Egloff <mattia.egloff@pm.me>
// SPDX-License-Identifier: GPL-3.0-or-later

use vauchi_gtk::core_ui::camera_preview::{PreviewBinding, PreviewFrame, preview_node_name};

// @internal
#[test]
fn preview_identity_matches_the_hardware_process_scope() {
    assert_eq!(
        preview_node_name("camera-42").unwrap(),
        format!("vauchi.camera.{}.63616d6572612d3432", std::process::id())
    );
    assert_eq!(preview_node_name(""), None);
    assert_eq!(preview_node_name(&"x".repeat(129)), None);
}

// @internal
#[test]
fn mapped_rgba_rows_are_packed_without_padding() {
    let frame =
        PreviewFrame::from_rgba(1, 2, 8, &[1, 2, 3, 255, 9, 9, 9, 9, 4, 5, 6, 255]).unwrap();
    assert_eq!((frame.width(), frame.height()), (1, 2));
    assert_eq!(frame.rgba(), &[1, 2, 3, 255, 4, 5, 6, 255]);
}

// @internal
#[test]
fn invalid_preview_extents_and_dimensions_are_rejected() {
    for (width, height, stride, bytes) in [
        (0, 1, 4, vec![0; 4]),
        (1, 0, 4, vec![0; 4]),
        (1921, 1, 1921 * 4, vec![0; 1921 * 4]),
        (1, 1081, 4, vec![0; 1081 * 4]),
        (1, 1, 3, vec![0; 4]),
        (1, 2, usize::MAX, vec![0; 4]),
        (1, 2, 4, vec![0; 7]),
    ] {
        assert_eq!(PreviewFrame::from_rgba(width, height, stride, &bytes), None);
    }
}

// @internal
#[test]
fn a_preview_binding_rejects_other_nodes_and_never_falls_back_after_removal() {
    let mut binding = PreviewBinding::new("camera-42").unwrap();
    let expected = preview_node_name("camera-42").unwrap();
    assert_eq!(binding.current(), None);
    assert!(!binding.observe(1, "webcam", "Video/Source", "101"));
    assert!(!binding.observe(2, &expected, "Audio/Source", "102"));
    assert!(binding.observe(3, &expected, "Video/Source", "103"));
    assert_eq!(binding.current(), Some((3, "103")));
    binding.remove(1);
    assert_eq!(binding.current(), Some((3, "103")));
    binding.remove(3);
    assert_eq!(binding.current(), None);
    assert!(binding.retired());
    assert!(!binding.observe(4, &expected, "Video/Source", "104"));
    assert_eq!(binding.current(), None);
}

// @internal
#[test]
fn a_descriptor_pins_both_node_id_and_serial_even_when_another_app_has_the_same_name() {
    let mut binding = PreviewBinding::for_source(42, 1042).unwrap();
    assert!(!binding.observe(41, "same-name", "Video/Source", "1042"));
    assert!(!binding.observe(42, "same-name", "Video/Source", "1043"));
    assert!(!binding.observe(42, "same-name", "Audio/Source", "1042"));
    assert!(binding.observe(42, "any-name", "Video/Source", "1042"));
    assert_eq!(binding.current(), Some((42, "1042")));
    binding.remove(42);
    assert!(binding.retired());
    assert!(!binding.observe(42, "any-name", "Video/Source", "1042"));
    assert!(PreviewBinding::for_source(u32::MAX, 1).is_none());
    assert!(PreviewBinding::for_source(42, 0).is_none());
}
