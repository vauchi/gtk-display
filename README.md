<!-- SPDX-FileCopyrightText: 2026 Mattia Egloff <mattia.egloff@pm.me> -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

> **Mirror:** This repo is a read-only mirror of [gitlab.com/vauchi/linux-gtk](https://gitlab.com/vauchi/linux-gtk). Please open issues and merge requests there.

[![Pipeline](https://img.shields.io/endpoint?url=https://vauchi.gitlab.io/linux-gtk/badges/pipeline.json&label=pipeline)](https://gitlab.com/vauchi/linux-gtk)
[![REUSE](https://api.reuse.software/badge/gitlab.com/vauchi/linux-gtk)](https://api.reuse.software/info/gitlab.com/vauchi/linux-gtk)

> [!NOTE]
> **You're early — and that's the point.** Vauchi is pre-alpha and
> under heavy development: not yet ready for production, and APIs may
> change without notice. If you're here now, you can help shape it —
> try it, break it, and tell us what's missing.

# Vauchi Linux GTK

The GTK4 renderer of Vauchi — living contact cards, exchanged in person.

A library, built with GTK4-rs and libadwaita: it renders Core's generic
presentation commands and reports events. The Linux app, `gvauchi`, wires
it to Core and ships the packages; it lives in
[linux-app](https://gitlab.com/vauchi/linux-app). The only Vauchi
dependency here is `vauchi-contract`; engine and hardware adapters live
in the app.

## Prerequisites

- GTK4 development libraries (`libgtk-4-dev`)
- libadwaita development libraries (`libadwaita-1-dev`)
- Rust 1.78+

## Build

```bash
cargo build
cargo test
```

## Architecture

This crate is a display-only shell: core emits generic, fully prepared
presentation commands, and the renderer reports opaque events back.

- **Generic surface** (`src/core_ui/generic_surface`) renders core's
  presentation commands with GTK4 widgets
- **Contextual surface** (`src/core_ui/contextual_surface`) maps core's
  back, navigation, primary, and secondary roles to native bars and
  overlays
- **Action dispatcher** (`src/core_ui/action_dispatcher`) reports user
  input to core as opaque events
- **Platform chrome**: HeaderBar, GNotification, libadwaita styling

All business logic lives in Core. This repo is a pure rendering layer,
tested against the contract's fixture corpus (`tests/contract_replay.rs`)
and through `render_fixture`, which renders a surface with no engine.

## License

GPL-3.0-or-later
