// SPDX-FileCopyrightText: 2026 Mattia Egloff <mattia.egloff@pm.me>
// SPDX-License-Identifier: GPL-3.0-or-later

//! Smoke test for the `render-fixture` catalog harness: a generic surface
//! fixture must render through the production renderer to a valid, non-trivial
//! PNG. Guards the design screenshot catalog
//! (`_private/docs/problems/2026-06-12-device-screenshot-catalog/`).
//!
//! Headless: runs the binary under Xvfb with the GDK x11 backend, mirroring
//! the existing AT-SPI snapshot tests. Skips (rather than fails) when no
//! display path is available, so a developer without Xvfb still gets a green
//! suite; CI provides Xvfb.

mod common;

use std::process::Command;
use vauchi_core::{
    AccessibilitySpec, PresentationNode, PresentationTextStyle, PresentationTokens, SurfaceId,
    SurfaceLayout, SurfaceSpec,
};

fn have(cmd: &str) -> bool {
    Command::new("sh")
        .args(["-c", &format!("command -v {cmd}")])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

// @internal — the native accessibility probe must decode with the current Core schema.
#[test]
fn accessibility_fixture_decodes_against_core() {
    let fixture: SurfaceSpec =
        serde_json::from_str(include_str!("atspi/fixtures/all_nodes.json")).unwrap();
    assert_eq!(fixture.surface_id.as_str(), "a11y_node_probe");
}

// @internal
#[test]
fn render_fixture_writes_valid_png() {
    let bin = env!("CARGO_BIN_EXE_render_fixture");
    let width = 900;
    let height = 1400;

    if !have("xvfb-run") {
        eprintln!("skip: xvfb-run not available — cannot render headlessly");
        return;
    }

    let output_dir = tempfile::tempdir().expect("create isolated render output directory");
    let fixture = output_dir.path().join("generic_surface.json");
    let out = output_dir.path().join("render_fixture_smoke.png");
    let surface = SurfaceSpec {
        surface_id: SurfaceId::new("fixture").expect("surface id"),
        revision: 1,
        title: "Generic surface".into(),
        subtitle: Some("Rendered from the canonical protocol".into()),
        accessibility_label: "Generic surface".into(),
        layout: SurfaceLayout::Scroll,
        tokens: PresentationTokens {
            spacing_small: 8,
            spacing_medium: 16,
            spacing_large: 24,
            corner_radius: 12,
            minimum_target_size: 44,
            ..common::presentation_tokens()
        },
        nodes: vec![
            PresentationNode::Text {
                id: None,
                content: "Prepared entirely by Core".into(),
                style: PresentationTextStyle::Heading,
                accessibility: AccessibilitySpec::label("Prepared entirely by Core"),
            },
            PresentationNode::Progress {
                label: Some("Migration progress".into()),
                value: Some(0.75),
                accessibility: AccessibilitySpec::label("Migration progress"),
            },
        ],
    };
    std::fs::write(
        &fixture,
        serde_json::to_vec_pretty(&surface).expect("serialize surface"),
    )
    .expect("write generic fixture");

    let status = Command::new("xvfb-run")
        .args([
            "-a",
            "-s",
            &format!("-screen 0 {}x{}x24", width + 80, height + 80),
            bin,
            fixture.to_str().unwrap(),
            out.to_str().unwrap(),
            &width.to_string(),
            &height.to_string(),
        ])
        .env("GDK_BACKEND", "x11")
        .status()
        .expect("spawn render-fixture under xvfb-run");
    assert!(status.success(), "render-fixture exited with {status}");

    let bytes = std::fs::read(&out).expect("read rendered png");
    assert!(
        bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
        "output is not a PNG (got {} bytes, magic {:02x?})",
        bytes.len(),
        &bytes[..bytes.len().min(8)]
    );
    assert!(
        bytes.len() > 2_000,
        "PNG suspiciously small ({} bytes) — likely a blank/unpainted frame",
        bytes.len()
    );
}
