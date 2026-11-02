// SPDX-FileCopyrightText: 2026 Mattia Egloff <mattia.egloff@pm.me>
// SPDX-License-Identifier: GPL-3.0-or-later

//! The camera path decodes with `vauchi_linux_hw::qr::scan_qr_from_luma`
//! (and `vauchi-core` still links its own copy of the decoder), whose only
//! crash guard lives in the `rqrr` fork in `vauchi/utils/rqrr`. A `[patch]`
//! applies solely inside the workspace that declares it, so without a stanza of our
//! own this crate resolves upstream `rqrr` from crates.io — unclamped — under
//! `panic = "abort"`. This test reads the lock file the binary was built from
//! and fails the moment `rqrr` resolves from the registry again — including
//! when a stale redirect leaves the fork as a dead second entry.
//! Record: `_private/docs/problems/2026-09-10-linux-gtk-unpatched-rqrr-abort-path/`.

const LOCK: &str = include_str!("../Cargo.lock");

fn rqrr_sources() -> Vec<String> {
    let mut sources = Vec::new();
    let mut in_rqrr = false;
    for line in LOCK.lines() {
        let line = line.trim();
        if line == "[[package]]" {
            in_rqrr = false;
        } else if line == "name = \"rqrr\"" {
            in_rqrr = true;
        } else if in_rqrr && let Some(rest) = line.strip_prefix("source = ") {
            sources.push(rest.trim_matches('"').to_string());
        }
    }
    sources
}

// @scenario: exchange :: A malformed QR frame never aborts the desktop shell
#[test]
fn rqrr_resolves_from_the_fork_not_the_registry() {
    let sources = rqrr_sources();
    assert_eq!(
        sources.len(),
        1,
        "exactly one rqrr must resolve; two means the redirect names a stale tag: {sources:?}"
    );
    assert!(
        sources[0]
            .starts_with("git+https://gitlab.com/vauchi/utils/rqrr.git?tag=v0.11.0%2Bvauchi."),
        "rqrr must come from the fork in vauchi/utils/rqrr by tag (clamped perspective), got: {}",
        sources[0]
    );
}
