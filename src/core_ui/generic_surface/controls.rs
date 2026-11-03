// SPDX-FileCopyrightText: 2026 Mattia Egloff <mattia.egloff@pm.me>
// SPDX-License-Identifier: GPL-3.0-or-later

use gtk4::prelude::*;
use gtk4::{Box as GtkBox, Label, Orientation, Widget};
use vauchi_contract::{
    AccessibilitySpec, BindingId, ChoiceOption, InputValue, PresentationNode, PresentationTokens,
    SurfaceId,
};

use super::{OnEvent, action_button, emit_binding_gesture, emit_value, targets};
use crate::core_ui::accessibility::{apply as apply_accessibility, mark_invalid};

pub(super) fn render(
    node: &PresentationNode,
    surface_id: &SurfaceId,
    on_event: &OnEvent,
    tokens: &PresentationTokens,
) -> Widget {
    match node {
        PresentationNode::Text {
            id,
            content,
            style,
            accessibility,
        } => {
            let label = Label::builder()
                .label(content)
                .halign(gtk4::Align::Start)
                .wrap(true)
                .build();
            if let Some(id) = id {
                label.set_widget_name(id.as_str());
            }
            apply_accessibility(&label, accessibility);
            match style {
                vauchi_contract::PresentationTextStyle::Heading => label.add_css_class("title-2"),
                vauchi_contract::PresentationTextStyle::Muted => label.add_css_class("dim-label"),
                vauchi_contract::PresentationTextStyle::Monospace => {
                    label.add_css_class("monospace")
                }
                _ => {}
            }
            label.upcast()
        }
        PresentationNode::Input {
            binding_id,
            label,
            value,
            placeholder,
            input_kind,
            max_length,
            validation_error,
            enabled,
            accessibility,
        } => {
            let group = GtkBox::new(Orientation::Vertical, 4);
            group.append(
                &Label::builder()
                    .label(label)
                    .halign(gtk4::Align::Start)
                    .build(),
            );
            let entry = gtk4::Entry::builder()
                .text(value)
                .placeholder_text(placeholder.as_deref().unwrap_or(""))
                .sensitive(*enabled)
                .visibility(!matches!(
                    input_kind,
                    vauchi_contract::PresentationInputKind::Password
                        | vauchi_contract::PresentationInputKind::Pin
                ))
                .build();
            entry.set_widget_name(binding_id.as_str());
            apply_accessibility(&entry, accessibility);
            if let Some(limit) = max_length.and_then(|value| i32::try_from(value).ok()) {
                entry.set_max_length(limit);
            }
            let id = binding_id.clone();
            let surface = surface_id.clone();
            let callback = on_event.clone();
            let changed_id = id.clone();
            let changed_surface = surface.clone();
            let changed_callback = callback.clone();
            entry.connect_changed(move |entry| {
                emit_value(
                    &changed_surface,
                    &changed_id,
                    InputValue::Text(entry.text().to_string()),
                    &changed_callback,
                );
            });
            let blur_id = id.clone();
            let blur_surface = surface.clone();
            let blur_callback = callback.clone();
            entry.connect_has_focus_notify(move |entry| {
                // The focus lands on the entry's inner `GtkText`, so the
                // entry's own `has_focus()` is false even while it is being
                // typed in — focus *arriving* looked like focus loss. And
                // an entry the shell has just dropped for a re-render
                // reports its loss after leaving the window; that is the
                // shell's doing, not the user leaving the field.
                if !entry.state_flags().contains(gtk4::StateFlags::FOCUS_WITHIN)
                    && entry.root().is_some()
                {
                    emit_value(
                        &blur_surface,
                        &blur_id,
                        InputValue::Text(entry.text().to_string()),
                        &blur_callback,
                    );
                    // GTK reports focus loss whatever took it, so no
                    // click-outside handling is needed here.
                    emit_binding_gesture(
                        vauchi_contract::Event::InputFocusEnded {
                            surface_id: blur_surface.clone(),
                            binding_id: blur_id.clone(),
                        },
                        &blur_callback,
                    );
                }
            });
            entry.connect_activate(move |entry| {
                emit_value(
                    &surface,
                    &id,
                    InputValue::Text(entry.text().to_string()),
                    &callback,
                );
                // `activate` is Enter in the entry — GTK's own submit
                // gesture.
                emit_binding_gesture(
                    vauchi_contract::Event::InputSubmitted {
                        surface_id: surface.clone(),
                        binding_id: id.clone(),
                    },
                    &callback,
                );
            });
            group.append(&entry);
            if let Some(error) = validation_error {
                let message = Label::builder()
                    .label(error)
                    .css_classes(["error"])
                    .halign(gtk4::Align::Start)
                    .build();
                mark_invalid(&entry, &message);
                group.append(&message);
            }
            group.upcast()
        }
        PresentationNode::Toggle {
            binding_id,
            label,
            value,
            enabled,
            accessibility,
        } => {
            let toggle = gtk4::CheckButton::builder()
                .label(label)
                .active(*value)
                .sensitive(*enabled)
                .build();
            toggle.set_widget_name(binding_id.as_str());
            toggle.set_size_request(-1, targets::minimum_target_px(tokens));
            toggle.add_css_class(targets::TARGET_RADIUS_CLASS);
            apply_accessibility(&toggle, accessibility);
            let id = binding_id.clone();
            let surface = surface_id.clone();
            let callback = on_event.clone();
            toggle.connect_toggled(move |toggle| {
                emit_value(
                    &surface,
                    &id,
                    InputValue::Boolean(toggle.is_active()),
                    &callback,
                );
            });
            toggle.upcast()
        }
        PresentationNode::Choice {
            binding_id,
            label,
            selected,
            options,
            enabled,
            accessibility,
        } => {
            let group = GtkBox::new(Orientation::Vertical, 4);
            group.append(
                &Label::builder()
                    .label(label)
                    .halign(gtk4::Align::Start)
                    .build(),
            );
            let choice = ChoiceControl {
                binding_id,
                selected: selected.as_deref(),
                options,
                enabled: *enabled,
                accessibility,
            };
            group.append(&match choice_widget(options.len()) {
                ChoiceWidget::Segmented => segmented_choice(&choice, surface_id, on_event, tokens),
                ChoiceWidget::DropDown => dropdown_choice(&choice, surface_id, on_event),
            });
            group.upcast()
        }
        PresentationNode::Confirmation {
            warning,
            confirm,
            cancel,
            accessibility,
            ..
        } => {
            let group = GtkBox::new(Orientation::Vertical, 8);
            apply_accessibility(&group, accessibility);
            group.append(&Label::builder().label(warning).wrap(true).build());
            let buttons = GtkBox::new(Orientation::Horizontal, 8);
            buttons.append(&action_button(cancel, surface_id, on_event, tokens));
            buttons.append(&action_button(confirm, surface_id, on_event, tokens));
            group.append(&buttons);
            group.upcast()
        }
        PresentationNode::Slider {
            binding_id,
            label,
            value,
            minimum,
            maximum,
            step,
            accessibility,
            ..
        } => {
            let group = GtkBox::new(Orientation::Vertical, 4);
            group.append(
                &Label::builder()
                    .label(label)
                    .halign(gtk4::Align::Start)
                    .build(),
            );
            let scale = gtk4::Scale::with_range(
                Orientation::Horizontal,
                *minimum,
                *maximum,
                step.unwrap_or(0.01),
            );
            scale.set_value(*value);
            scale.set_widget_name(binding_id.as_str());
            apply_accessibility(&scale, accessibility);
            let id = binding_id.clone();
            let surface = surface_id.clone();
            let callback = on_event.clone();
            scale.connect_value_changed(move |scale| {
                emit_value(&surface, &id, InputValue::Number(scale.value()), &callback);
            });
            group.append(&scale);
            group.upcast()
        }
        PresentationNode::Progress {
            label,
            value,
            accessibility,
        } => {
            let progress = gtk4::ProgressBar::new();
            progress.set_text(label.as_deref());
            progress.set_show_text(label.is_some());
            value.map_or_else(|| progress.pulse(), |value| progress.set_fraction(value));
            apply_accessibility(&progress, accessibility);
            progress.upcast()
        }
        _ => Label::new(None).upcast(),
    }
}

/// The design canvas draws two- and three-way choices as a segmented
/// control; anything longer stays a drop-down so Settings' 15-entry theme
/// list does not become a row of buttons.
const SEGMENTED_OPTION_COUNTS: std::ops::RangeInclusive<usize> = 2..=3;

#[derive(Debug, PartialEq, Eq)]
enum ChoiceWidget {
    Segmented,
    DropDown,
}

fn choice_widget(option_count: usize) -> ChoiceWidget {
    if SEGMENTED_OPTION_COUNTS.contains(&option_count) {
        ChoiceWidget::Segmented
    } else {
        ChoiceWidget::DropDown
    }
}

fn choice_value(options: &[ChoiceOption], index: usize) -> InputValue {
    InputValue::Choice(options.get(index).map(|option| option.id.clone()))
}

struct ChoiceControl<'a> {
    binding_id: &'a BindingId,
    selected: Option<&'a str>,
    options: &'a [ChoiceOption],
    enabled: bool,
    accessibility: &'a AccessibilitySpec,
}

fn segmented_choice(
    choice: &ChoiceControl<'_>,
    surface_id: &SurfaceId,
    on_event: &OnEvent,
    tokens: &PresentationTokens,
) -> Widget {
    let segments = GtkBox::new(Orientation::Horizontal, 0);
    segments.add_css_class("linked");
    segments.set_sensitive(choice.enabled);
    segments.set_widget_name(choice.binding_id.as_str());
    apply_accessibility(&segments, choice.accessibility);
    let mut first_segment: Option<gtk4::ToggleButton> = None;
    for (index, option) in choice.options.iter().enumerate() {
        let segment = gtk4::ToggleButton::builder()
            .label(&option.label)
            .active(choice.selected == Some(option.id.as_str()))
            .hexpand(true)
            .build();
        segment.set_size_request(-1, targets::minimum_target_px(tokens));
        match &first_segment {
            Some(first) => segment.set_group(Some(first)),
            None => first_segment = Some(segment.clone()),
        }
        let options = choice.options.to_vec();
        let id = choice.binding_id.clone();
        let surface = surface_id.clone();
        let callback = on_event.clone();
        segment.connect_toggled(move |segment| {
            // The group fires `toggled` on the segment that just went
            // inactive too; only the newly active one is the choice.
            if segment.is_active() {
                emit_value(&surface, &id, choice_value(&options, index), &callback);
            }
        });
        segments.append(&segment);
    }
    segments.upcast()
}

fn dropdown_choice(
    choice: &ChoiceControl<'_>,
    surface_id: &SurfaceId,
    on_event: &OnEvent,
) -> Widget {
    let labels = choice
        .options
        .iter()
        .map(|option| option.label.as_str())
        .collect::<Vec<_>>();
    let dropdown = gtk4::DropDown::from_strings(&labels);
    dropdown.set_sensitive(choice.enabled);
    dropdown.set_widget_name(choice.binding_id.as_str());
    apply_accessibility(&dropdown, choice.accessibility);
    if let Some(index) = choice.selected.and_then(|selected| {
        choice
            .options
            .iter()
            .position(|option| option.id == selected)
    }) {
        dropdown.set_selected(index as u32);
    }
    let options = choice.options.to_vec();
    let id = choice.binding_id.clone();
    let surface = surface_id.clone();
    let callback = on_event.clone();
    dropdown.connect_selected_notify(move |dropdown| {
        emit_value(
            &surface,
            &id,
            choice_value(&options, dropdown.selected() as usize),
            &callback,
        );
    });
    dropdown.upcast()
}

// INLINE_TEST_REQUIRED: tests exercise private-to-the-crate helpers with no
// public accessor, and cannot be reached through a widget without a display.
#[cfg(test)]
mod choice_tests {
    use super::{ChoiceWidget, choice_value, choice_widget};
    use vauchi_contract::{ChoiceOption, InputValue};

    fn options(count: usize) -> Vec<ChoiceOption> {
        (1..=count)
            .map(|index| ChoiceOption {
                id: format!("option_{index}"),
                label: format!("Option {index}"),
            })
            .collect()
    }

    /// The design canvas draws two- and three-way choices (contact-detail
    /// Perspective, groups Members/Visibility) as a segmented control; a
    /// single option has nothing to switch between and Settings' 15-entry
    /// theme list would not fit in a row of buttons.
    // @internal
    #[test]
    fn two_and_three_options_render_segmented_everything_else_drops_down() {
        assert_eq!(choice_widget(0), ChoiceWidget::DropDown);
        assert_eq!(choice_widget(1), ChoiceWidget::DropDown);
        assert_eq!(choice_widget(2), ChoiceWidget::Segmented);
        assert_eq!(choice_widget(3), ChoiceWidget::Segmented);
        assert_eq!(choice_widget(4), ChoiceWidget::DropDown);
        assert_eq!(choice_widget(15), ChoiceWidget::DropDown);
    }

    // @internal
    #[test]
    fn the_activated_segment_emits_its_option_id_as_the_choice_value() {
        let options = options(3);

        assert_eq!(
            choice_value(&options, 1),
            InputValue::Choice(Some("option_2".to_string()))
        );
    }

    // @internal
    #[test]
    fn a_segment_index_past_the_options_emits_no_choice() {
        assert_eq!(choice_value(&options(2), 2), InputValue::Choice(None));
    }
}
