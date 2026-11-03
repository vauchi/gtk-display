// SPDX-FileCopyrightText: 2026 Mattia Egloff <mattia.egloff@pm.me>
// SPDX-License-Identifier: GPL-3.0-or-later

//! Persistent navigation sidebar (F6/D4).
//!
//! Core's `SetNavigation` command publishes the same destinations the
//! navigation overlay presents, but alongside every surface rather than
//! only when a context-bar button opens them. A nonempty list renders as a
//! native `AdwOverlaySplitView` sidebar instead of a hidden destination
//! list behind a modal open-and-dismiss; `overlays::present` keeps the
//! modal as the narrow-window fallback for when the split view collapses.

use gtk4::prelude::*;
use gtk4::{
    AccessibleRole, Align, Box as GtkBox, Button, Image, Label, ListBox, ListBoxRow, Orientation,
    PolicyType, ScrolledWindow, SelectionMode,
};
use libadwaita as adw;

use vauchi_contract::{InteractionId, NavigationItem, NavigationSpec};

use crate::core_ui::engine_link::EngineHandle;

use crate::core_ui::accessibility;
use crate::core_ui::navigation_icons::resolve_icon_name;

use super::widgets::{COMMAND_STATE, dispatch_interaction};

pub(super) const SPLIT_VIEW_NAME: &str = "persistent-navigation-split-view";
const WRAPPER_NAME: &str = "persistent-navigation-sidebar";
const LIST_NAME: &str = "persistent-navigation-list";

/// Pure projection of a `NavigationSpec` into what the sidebar needs to
/// draw: which rows, in order, and which one (if any) is selected. Kept
/// free of GTK types so the shape is testable without a display.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct SidebarModel {
    pub items: Vec<NavigationItem>,
    pub selected_index: Option<usize>,
}

impl SidebarModel {
    pub fn from_navigation(navigation: &NavigationSpec) -> Self {
        Self {
            selected_index: navigation.items.iter().position(|item| item.selected),
            items: navigation.items.clone(),
        }
    }

    pub fn is_hidden(&self) -> bool {
        self.items.is_empty()
    }
}

/// Build the `AdwOverlaySplitView` that carries the persistent sidebar.
///
/// `command_target` is the box Core's rendered surfaces and context bar
/// write into — the same one every other dispatch call site uses — so a
/// row activation reaches Core through the identical path a context-bar
/// button does.
pub fn build_split_view(
    command_target: &GtkBox,
    app_engine: &EngineHandle,
    toast_overlay: &adw::ToastOverlay,
) -> adw::OverlaySplitView {
    let split_view = adw::OverlaySplitView::new();
    split_view.set_widget_name(SPLIT_VIEW_NAME);
    split_view.set_vexpand(true);
    split_view.set_sidebar_width_fraction(0.25);
    split_view.set_min_sidebar_width(180.0);
    split_view.set_max_sidebar_width(280.0);
    split_view.set_content(Some(toast_overlay));
    split_view.set_sidebar(Some(&build_wrapper(
        command_target,
        app_engine,
        toast_overlay,
    )));
    split_view
}

fn build_wrapper(
    command_target: &GtkBox,
    app_engine: &EngineHandle,
    toast_overlay: &adw::ToastOverlay,
) -> GtkBox {
    let wrapper = GtkBox::new(Orientation::Vertical, 0);
    wrapper.set_widget_name(WRAPPER_NAME);
    wrapper.add_css_class("nav-sidebar");
    // Nothing to show until the first `SetNavigation` lands.
    wrapper.set_visible(false);

    let list = ListBox::new();
    list.set_widget_name(LIST_NAME);
    list.set_selection_mode(SelectionMode::None);
    list.add_css_class("nav-sidebar-list");

    let command_target = command_target.clone();
    let app_engine = app_engine.clone();
    let toast_overlay = toast_overlay.clone();
    list.connect_row_activated(move |_, row| {
        let Ok(interaction_id) = InteractionId::new(row.widget_name().as_str()) else {
            return;
        };
        let surface_id = COMMAND_STATE.with(|state| {
            state
                .borrow()
                .navigation()
                .map(|(surface_id, _)| surface_id.clone())
        });
        let Some(surface_id) = surface_id else {
            return;
        };
        // Same send path the navigation overlay's buttons use
        // (`overlays::present_navigation`): no origin, since this row is
        // not a context-bar control an overlay needs to re-anchor to.
        dispatch_interaction(
            &command_target,
            &app_engine,
            &toast_overlay,
            &surface_id,
            &interaction_id,
            None,
        );
    });

    let scroller = ScrolledWindow::builder()
        .hscrollbar_policy(PolicyType::Never)
        .vscrollbar_policy(PolicyType::Automatic)
        .child(&list)
        .build();
    scroller.set_vexpand(true);
    wrapper.append(&scroller);
    wrapper
}

/// Rebuild the sidebar rows from Core's latest `NavigationSpec`, or hide the
/// sidebar entirely when there is none to show — a locked app sends empty
/// items, and a shell that never built the split view (`render_fixture`'s
/// offscreen harness) simply has nothing to find.
pub(super) fn update(command_target: &GtkBox, navigation: Option<&NavigationSpec>) {
    let Some(root) = command_target.root() else {
        return;
    };
    let root: gtk4::Widget = root.upcast();
    let model = navigation
        .map(SidebarModel::from_navigation)
        .unwrap_or_default();

    if let Some(wrapper) = super::commands::find_widget_by_name(&root, WRAPPER_NAME) {
        wrapper.set_visible(!model.is_hidden());
    }
    let Some(list_widget) = super::commands::find_widget_by_name(&root, LIST_NAME) else {
        return;
    };
    let Ok(list) = list_widget.downcast::<ListBox>() else {
        return;
    };
    while let Some(row) = list.first_child() {
        list.remove(&row);
    }
    for (index, item) in model.items.iter().enumerate() {
        list.append(&build_row(item, Some(index) == model.selected_index));
    }
}

/// Collapse the split view to a single column, or restore the sidebar
/// pane. Driven by Core's `WindowClass` (`environment::apply_window_class`)
/// rather than a shell-computed width — a window without the split view
/// wired (again, `render_fixture`) simply has nothing to find.
pub(super) fn set_collapsed(command_target: &GtkBox, collapsed: bool) {
    let Some(root) = command_target.root() else {
        return;
    };
    let root: gtk4::Widget = root.upcast();
    if let Some(split_view) = super::commands::find_widget_by_name(&root, SPLIT_VIEW_NAME)
        .and_then(|widget| widget.downcast::<adw::OverlaySplitView>().ok())
    {
        split_view.set_collapsed(collapsed);
    }
}

fn build_row(item: &NavigationItem, selected: bool) -> ListBoxRow {
    let row = ListBoxRow::new();
    row.set_widget_name(item.interaction_id.as_str());
    // The button below is the focus stop and the accessible tab; a list row
    // exposes no AT-SPI action, so assistive tech could see a destination
    // but not choose it (2026-05-16-linux-gtk-atspi-sidebar-navigate).
    row.set_focusable(false);
    row.add_css_class("vauchi-row");
    row.add_css_class("nav-sidebar-row");
    if selected {
        row.add_css_class("selected");
    }

    let button = Button::builder()
        .accessible_role(AccessibleRole::Tab)
        .build();
    button.add_css_class("flat");
    accessibility::apply_label(&button, &item.accessibility_label);
    let activated_row = row.downgrade();
    button.connect_clicked(move |_| {
        if let Some(row) = activated_row.upgrade() {
            row.activate();
        }
    });

    let content = GtkBox::new(Orientation::Horizontal, 8);
    content.set_margin_top(8);
    content.set_margin_bottom(8);
    content.set_margin_start(12);
    content.set_margin_end(12);

    let theme = crate::core_ui::pictograms::icon_theme();
    let icon_name = resolve_icon_name(item.icon_token.as_deref(), |name| {
        theme.as_ref().is_some_and(|theme| theme.has_icon(name))
    });
    content.append(&Image::from_icon_name(&icon_name));

    let label = Label::new(Some(&item.label));
    label.set_hexpand(true);
    label.set_halign(Align::Start);
    content.append(&label);

    if item.badge_count > 0 {
        let badge = Label::new(Some(&item.badge_count.to_string()));
        badge.add_css_class("nav-sidebar-badge");
        content.append(&badge);
    }

    button.set_child(Some(&content));
    row.set_child(Some(&button));
    row
}

// INLINE_TEST_REQUIRED: tests exercise the private `SidebarModel` projection
#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: &str, selected: bool, badge_count: u32) -> NavigationItem {
        NavigationItem {
            interaction_id: InteractionId::new(id).unwrap(),
            label: id.to_owned(),
            accessibility_label: id.to_owned(),
            icon_token: None,
            selected,
            badge_count,
        }
    }

    // @internal
    #[test]
    fn model_lists_rows_in_core_order() {
        let navigation = NavigationSpec {
            items: vec![item("first", false, 0), item("second", true, 3)],
        };

        let model = SidebarModel::from_navigation(&navigation);

        assert_eq!(
            model
                .items
                .iter()
                .map(|item| item.interaction_id.as_str())
                .collect::<Vec<_>>(),
            vec!["first", "second"],
            "rows must render in the order Core sent them"
        );
    }

    // @internal
    #[test]
    fn model_finds_the_selected_index() {
        let navigation = NavigationSpec {
            items: vec![item("first", false, 0), item("second", true, 0)],
        };

        let model = SidebarModel::from_navigation(&navigation);

        assert_eq!(
            model.selected_index,
            Some(1),
            "the selected row's position must be reported so the widget \
             layer can highlight it without re-deriving Core's choice"
        );
    }

    // @internal
    #[test]
    fn model_has_no_selected_index_when_core_selects_none() {
        let navigation = NavigationSpec {
            items: vec![item("first", false, 0), item("second", false, 0)],
        };

        let model = SidebarModel::from_navigation(&navigation);

        assert_eq!(model.selected_index, None);
    }

    // @internal
    #[test]
    fn model_is_hidden_when_navigation_has_no_items() {
        let model = SidebarModel::from_navigation(&NavigationSpec { items: Vec::new() });

        assert!(
            model.is_hidden(),
            "a locked app or unsupported surface sends empty items, and the \
             sidebar must disappear rather than render with nothing in it"
        );
    }

    // @internal
    #[test]
    fn model_is_not_hidden_with_at_least_one_item() {
        let model = SidebarModel::from_navigation(&NavigationSpec {
            items: vec![item("first", true, 0)],
        });

        assert!(!model.is_hidden());
    }
}
