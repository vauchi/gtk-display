// SPDX-FileCopyrightText: 2026 Mattia Egloff <mattia.egloff@pm.me>
// SPDX-License-Identifier: GPL-3.0-or-later

//! Input decoding for the `render-catalog` harness.
//!
//! Core publishes every app screen as a replayable command batch in
//! `vauchi-app/fixtures/screen_catalog_v1.json`. Until that catalog exists
//! on Core `main`, the presentation contract fixture carries batches of the
//! same shape (`initial_commands` plus one per step), so both decode to the
//! same list of render jobs and CI reports which one it drew from.

use vauchi_contract::Command;

/// Which fixture shape produced the render jobs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CatalogSource {
    ScreenCatalog,
    PresentationContract,
}

impl CatalogSource {
    pub fn describe(self) -> &'static str {
        match self {
            Self::ScreenCatalog => "screen catalog (screens[])",
            Self::PresentationContract => {
                "presentation contract fallback (initial_commands + steps[])"
            }
        }
    }
}

/// One screen to render: the PNG stem and the batch that paints it.
#[derive(Clone, Debug, PartialEq)]
pub struct CatalogScreen {
    pub code_id: String,
    pub commands: Vec<Command>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LoadedCatalog {
    pub source: CatalogSource,
    pub screens: Vec<CatalogScreen>,
}

/// Decode either fixture shape. Errors name the offending field so a CI
/// trace points at the catalog entry, not at serde internals.
pub fn load_screens(json: &str) -> Result<LoadedCatalog, String> {
    let root: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("catalog is not JSON: {e}"))?;
    let (source, screens) = if let Some(entries) = root.get("screens") {
        (CatalogSource::ScreenCatalog, catalog_screens(entries)?)
    } else if let Some(initial) = root.get("initial_commands") {
        (
            CatalogSource::PresentationContract,
            contract_screens(initial, root.get("steps"))?,
        )
    } else {
        return Err(
            "catalog has neither a `screens` array nor `initial_commands`: unknown shape".into(),
        );
    };
    reject_duplicate_stems(&screens)?;
    Ok(LoadedCatalog { source, screens })
}

fn catalog_screens(entries: &serde_json::Value) -> Result<Vec<CatalogScreen>, String> {
    let entries = entries
        .as_array()
        .ok_or_else(|| "`screens` must be an array".to_string())?;
    entries
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            let code_id = entry
                .get("code_id")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| format!("screens[{index}]: missing string `code_id`"))?;
            validate_stem(code_id).map_err(|why| format!("screens[{index}].code_id: {why}"))?;
            let commands = decode_batch(entry.get("commands"))
                .map_err(|why| format!("screens[{index}] ({code_id}): {why}"))?;
            Ok(CatalogScreen {
                code_id: code_id.to_string(),
                commands,
            })
        })
        .collect()
}

fn contract_screens(
    initial: &serde_json::Value,
    steps: Option<&serde_json::Value>,
) -> Result<Vec<CatalogScreen>, String> {
    let mut screens = vec![CatalogScreen {
        code_id: "initial".into(),
        commands: decode_batch(Some(initial)).map_err(|why| format!("initial_commands: {why}"))?,
    }];
    let steps = steps
        .and_then(serde_json::Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    for (index, step) in steps.iter().enumerate() {
        let number = index + 1;
        screens.push(CatalogScreen {
            code_id: format!("step_{number}"),
            commands: decode_batch(step.get("commands"))
                .map_err(|why| format!("steps[{index}]: {why}"))?,
        });
    }
    Ok(screens)
}

fn decode_batch(value: Option<&serde_json::Value>) -> Result<Vec<Command>, String> {
    let value = value.ok_or_else(|| "missing `commands` array".to_string())?;
    serde_json::from_value(value.clone())
        .map_err(|e| format!("`commands` is not a Core batch: {e}"))
}

/// The stem becomes `<out-dir>/<stem>.png`, so it must be a single plain
/// path component: no separators, no leading dot, no traversal.
fn validate_stem(stem: &str) -> Result<(), String> {
    const MAX_LEN: usize = 64;
    let mut chars = stem.chars();
    match chars.next() {
        None => return Err("must not be empty".into()),
        Some(first) if !first.is_ascii_alphanumeric() => {
            return Err(format!(
                "must start with an ASCII letter or digit, got {first:?}"
            ));
        }
        Some(_) => {}
    }
    if stem.len() > MAX_LEN {
        return Err(format!("must be at most {MAX_LEN} bytes"));
    }
    if let Some(bad) = chars.find(|c| !(c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.')))
    {
        return Err(format!(
            "may only contain ASCII letters, digits, '_', '-' and '.', got {bad:?}"
        ));
    }
    Ok(())
}

fn reject_duplicate_stems(screens: &[CatalogScreen]) -> Result<(), String> {
    let mut seen = std::collections::HashSet::new();
    for screen in screens {
        if !seen.insert(screen.code_id.as_str()) {
            return Err(format!(
                "code_id {:?} appears twice — the second PNG would overwrite the first",
                screen.code_id
            ));
        }
    }
    Ok(())
}
