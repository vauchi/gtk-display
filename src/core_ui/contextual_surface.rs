// SPDX-FileCopyrightText: 2026 Mattia Egloff <mattia.egloff@pm.me>
// SPDX-License-Identifier: GPL-3.0-or-later

//! Humble GTK state adapter for Core presentation commands.

mod commands;
mod environment;
mod overlays;
mod sidebar;
mod widgets;

pub(crate) use commands::handle_commands;
pub(crate) use environment::install_environment_reporting;
pub use sidebar::build_split_view;
pub use widgets::commands_for_event;
pub(crate) use widgets::{
    dispatch_platform_event, dispatch_shortcut, render_current_surface, request_back,
};

use crate::core_ui::engine_link::EngineHandle;
use std::collections::HashMap;

use libadwaita as adw;

/// Replay one Core batch from a cold presentation state, exactly as the
/// app applies it (`handle_commands`: surfaces, context bar, sidebar).
///
/// Harness entry for `render-catalog`: every catalog screen is captured
/// from the same blank slate, so revisions of an earlier screen never
/// veto a later one and nothing leaks between captures.
pub fn replay_command_batch(
    container: &gtk4::Box,
    app_engine: &EngineHandle,
    toast_overlay: &adw::ToastOverlay,
    commands: Vec<Command>,
) {
    widgets::COMMAND_STATE.with(|state| *state.borrow_mut() = GtkPresentationState::default());
    handle_commands(container, app_engine, toast_overlay, commands, None);
}
use vauchi_core::{
    ActionSpec, Command, ContextBar, Event, InteractionId, MotionPreference, NavigationSpec,
    OverlayKind, OverlaySpec, PaneLayout, PresentationProfile, StandardShortcut, SurfaceId,
    SurfaceSpec,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GtkContextRole {
    Back,
    Navigation,
    Primary,
    Secondary,
    Info,
}

/// Where native GTK code draws a role's control now that the separate row
/// above the content is retired: every role but the full-width action moves
/// into the surface's header bar (vauchi/private#534).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GtkControlArea {
    TitleRowLeading,
    TitleRowTrailing,
    SurfaceContent,
}

impl GtkContextRole {
    pub fn area(self) -> GtkControlArea {
        match self {
            GtkContextRole::Back | GtkContextRole::Navigation => GtkControlArea::TitleRowLeading,
            GtkContextRole::Secondary | GtkContextRole::Info => GtkControlArea::TitleRowTrailing,
            GtkContextRole::Primary => GtkControlArea::SurfaceContent,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct GtkContextControl<'a> {
    pub role: GtkContextRole,
    pub action: &'a ActionSpec,
    pub emphasized: bool,
}

pub fn context_controls(bar: &ContextBar) -> Vec<GtkContextControl<'_>> {
    context_controls_with(bar, false)
}

/// The controls to draw. Beside a sidebar the navigation launcher is left
/// out: both open the same destinations, and two controls for one list
/// was what readers could not name (vauchi/private#479).
pub fn context_controls_with(
    bar: &ContextBar,
    navigation_shown: bool,
) -> Vec<GtkContextControl<'_>> {
    let navigation = if navigation_shown {
        None
    } else {
        bar.navigation.as_ref()
    };
    [
        (GtkContextRole::Back, bar.back.as_ref()),
        (GtkContextRole::Navigation, navigation),
        (GtkContextRole::Primary, bar.primary.as_ref()),
        (GtkContextRole::Secondary, bar.secondary.as_ref()),
        (GtkContextRole::Info, bar.info.as_ref()),
    ]
    .into_iter()
    .filter_map(|(role, action)| {
        action.map(|action| GtkContextControl {
            role,
            action,
            emphasized: role == GtkContextRole::Primary,
        })
    })
    .collect()
}

pub fn interaction_for_shortcut(
    bar: &ContextBar,
    shortcut: StandardShortcut,
) -> Option<&InteractionId> {
    context_controls(bar)
        .into_iter()
        .find(|control| control.action.shortcut == Some(shortcut))
        .map(|control| &control.action.interaction_id)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GtkOverlayTransition {
    NavigationSlide,
    NavigationReveal,
    ActionScale,
    ActionCrossfade,
}

impl GtkOverlayTransition {
    pub fn for_overlay(kind: OverlayKind, motion: MotionPreference) -> Self {
        match (kind, motion) {
            (OverlayKind::Navigation, MotionPreference::Full) => Self::NavigationSlide,
            (OverlayKind::Navigation, MotionPreference::Reduced) => Self::NavigationReveal,
            (OverlayKind::ActionMenu, MotionPreference::Full) => Self::ActionScale,
            (OverlayKind::ActionMenu, MotionPreference::Reduced) => Self::ActionCrossfade,
            _ => Self::ActionCrossfade,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct GtkPresentationState {
    surfaces: HashMap<SurfaceId, SurfaceSpec>,
    context_bars: HashMap<SurfaceId, (u64, ContextBar)>,
    profile: Option<PresentationProfile>,
    overlays: HashMap<SurfaceId, (u64, OverlaySpec)>,
    navigation: HashMap<SurfaceId, (u64, NavigationSpec)>,
    last_surface: Option<SurfaceId>,
}

impl GtkPresentationState {
    pub fn apply(&mut self, command: Command) -> bool {
        match command {
            Command::ReplaceSurface { surface } => {
                let replace = self
                    .surfaces
                    .get(&surface.surface_id)
                    .is_none_or(|current| surface.revision >= current.revision);
                if replace {
                    let surface_id = surface.surface_id.clone();
                    self.surfaces.insert(surface_id.clone(), surface);
                    self.context_bars.remove(&surface_id);
                    self.overlays.remove(&surface_id);
                    self.navigation.remove(&surface_id);
                    self.last_surface = Some(surface_id);
                }
                replace
            }
            Command::SetContextBar {
                surface_id,
                revision,
                bar,
            } if self.is_current_revision(&surface_id, revision) => {
                self.context_bars.insert(surface_id, (revision, *bar));
                true
            }
            Command::SetNavigation {
                surface_id,
                revision,
                navigation,
            } if self.is_current_revision(&surface_id, revision) => {
                self.navigation.insert(surface_id, (revision, navigation));
                true
            }
            Command::SetPresentationProfile { profile } => {
                self.profile = Some(profile);
                true
            }
            Command::PresentOverlay {
                surface_id,
                revision,
                overlay,
            } if self.is_current_revision(&surface_id, revision) => {
                self.overlays.insert(surface_id, (revision, overlay));
                true
            }
            // Core rewrites a repeat PresentOverlay into this so the
            // context-bar buttons toggle. Matching on kind as well as surface
            // keeps a stale dismiss from closing an overlay Core has since
            // replaced.
            Command::DismissOverlay {
                surface_id, kind, ..
            } if self
                .overlays
                .get(&surface_id)
                .is_some_and(|(_, open)| open.kind == kind) =>
            {
                self.overlays.remove(&surface_id);
                true
            }
            Command::SetContextBar { .. }
            | Command::PresentOverlay { .. }
            | Command::DismissOverlay { .. }
            | Command::SetNavigation { .. } => false,
            _ => true,
        }
    }

    pub fn context_bar(&self) -> Option<(&SurfaceId, &ContextBar)> {
        let surface_id = self.active_surface_id()?;
        self.context_bars
            .get_key_value(surface_id)
            .map(|(surface, (_, bar))| (surface, bar))
    }

    pub fn navigation(&self) -> Option<(&SurfaceId, &NavigationSpec)> {
        let surface_id = self.active_surface_id()?;
        self.navigation
            .get_key_value(surface_id)
            .map(|(surface, (_, navigation))| (surface, navigation))
    }

    pub fn surface(&self) -> Option<&SurfaceSpec> {
        self.active_surface_id()
            .and_then(|surface_id| self.surfaces.get(surface_id))
    }

    pub fn surface_by_id(&self, surface_id: &SurfaceId) -> Option<&SurfaceSpec> {
        self.surfaces.get(surface_id)
    }

    pub fn visible_surface_ids(&self) -> Vec<SurfaceId> {
        let Some(profile) = &self.profile else {
            return self.last_surface.clone().into_iter().collect();
        };
        match profile.pane_layout {
            PaneLayout::Split => std::iter::once(&profile.primary_surface)
                .chain(profile.detail_surface.as_ref())
                .filter(|surface_id| self.surfaces.contains_key(*surface_id))
                .cloned()
                .collect(),
            PaneLayout::Single => {
                let surface_id = if self.surfaces.contains_key(&profile.active_surface) {
                    &profile.active_surface
                } else {
                    &profile.primary_surface
                };
                self.surfaces
                    .contains_key(surface_id)
                    .then(|| surface_id.clone())
                    .into_iter()
                    .collect()
            }
        }
    }

    pub fn visible_surfaces(&self) -> Vec<&SurfaceSpec> {
        self.visible_surface_ids()
            .iter()
            .filter_map(|surface_id| self.surfaces.get(surface_id))
            .collect()
    }

    pub fn profile(&self) -> Option<&PresentationProfile> {
        self.profile.as_ref()
    }

    pub fn overlay(&self) -> Option<(&SurfaceId, &OverlaySpec)> {
        let surface_id = self.active_surface_id()?;
        self.overlays
            .get_key_value(surface_id)
            .map(|(surface, (_, overlay))| (surface, overlay))
    }

    pub fn take_overlay(&mut self) -> Option<(SurfaceId, OverlaySpec)> {
        let surface_id = self.active_surface_id()?.clone();
        self.overlays
            .remove(&surface_id)
            .map(|(_, overlay)| (surface_id, overlay))
    }

    pub fn activation_events(&self, interaction_id: &InteractionId) -> Vec<Event> {
        let Some((surface_id, _)) = self.context_bar() else {
            return Vec::new();
        };
        vec![
            Event::SurfaceActivated {
                surface_id: surface_id.clone(),
            },
            Event::ActionActivated {
                surface_id: surface_id.clone(),
                interaction_id: interaction_id.clone(),
            },
        ]
    }

    fn is_current_revision(&self, surface_id: &SurfaceId, revision: u64) -> bool {
        self.surfaces
            .get(surface_id)
            .is_some_and(|surface| surface.revision == revision)
    }

    fn active_surface_id(&self) -> Option<&SurfaceId> {
        self.profile
            .as_ref()
            .map(|profile| &profile.active_surface)
            .filter(|surface_id| self.surfaces.contains_key(*surface_id))
            .or(self.last_surface.as_ref())
    }
}
