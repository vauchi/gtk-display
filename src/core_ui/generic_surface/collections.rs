// SPDX-FileCopyrightText: 2026 Mattia Egloff <mattia.egloff@pm.me>
// SPDX-License-Identifier: GPL-3.0-or-later

use gtk4::prelude::*;
use gtk4::{Box as GtkBox, Button, DrawingArea, Entry, Label, Orientation, Widget};
use libadwaita as adw;
use vauchi_contract::{
    ActionSpec, BindingId, InputValue, PresentationImageShape, PresentationNode,
    PresentationQrErrorCorrection, PresentationQrPurpose, PresentationRow, PresentationTokens,
    QrPlacement, SurfaceId,
};

use super::{OnEvent, action_button, emit_value, render_node, targets};
use crate::core_ui::accessibility::apply as apply_accessibility;
use crate::core_ui::engine_link::EngineHandle;
use crate::core_ui::navigation_icons::content_pictogram_name;

/// What an `Image` node resolves to, decided before any widget exists so the
/// choice can be asserted without a display.
///
/// The previous arm destructured `fallback_text, activation, accessibility,
/// ..` — and the `..` swallowed `data`. A user who had set an avatar saw
/// their initials on this shell, always, because the bytes were never
/// decoded by anything.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ImageContent<'a> {
    Picture(&'a [u8]),
    Initials(&'a str),
    Nothing,
}

pub(crate) fn image_content<'a>(
    data: Option<&'a [u8]>,
    fallback_text: Option<&'a str>,
) -> ImageContent<'a> {
    match (data, fallback_text) {
        (Some(bytes), _) if !bytes.is_empty() => ImageContent::Picture(bytes),
        (_, Some(initials)) if !initials.is_empty() => ImageContent::Initials(initials),
        _ => ImageContent::Nothing,
    }
}

/// Core's `shape` decides whether the result is round. `Circle` maps onto
/// libadwaita's own avatar widget (`avatar_widget`); `Natural`, and any
/// shape this build has not learned, keeps its own corners instead —
/// cropping content is the lossy choice, so an unrecognized shape falls to
/// the conservative side.
fn shape_is_circle(shape: PresentationImageShape) -> bool {
    matches!(shape, PresentationImageShape::Circle)
}

fn shape_classes(shape: PresentationImageShape) -> Vec<&'static str> {
    if shape_is_circle(shape) {
        vec!["avatar"]
    } else {
        vec!["avatar", "natural"]
    }
}

/// `Circle` content through libadwaita's own avatar widget: it draws the
/// picture-or-initials decision and the round crop in one place, which is
/// the treatment this build cannot get from a plain `Picture`/`Label` pair.
fn avatar_widget(content: &ImageContent<'_>, target_px: i32) -> Widget {
    let initials = match content {
        ImageContent::Initials(text) => Some(*text),
        _ => None,
    };
    let avatar = adw::Avatar::new(target_px, initials, true);
    if let ImageContent::Picture(bytes) = content {
        let gbytes = gtk4::glib::Bytes::from(*bytes);
        if let Ok(texture) = gtk4::gdk::Texture::from_bytes(&gbytes) {
            avatar.set_custom_image(Some(&texture));
        }
    }
    avatar.upcast()
}

fn picture_widget(bytes: &[u8], shape: PresentationImageShape) -> Widget {
    let gbytes = gtk4::glib::Bytes::from(bytes);
    gtk4::gdk::Texture::from_bytes(&gbytes).map_or_else(
        |_| {
            // Undecodable bytes are not an avatar. Falling through to
            // an empty label keeps the surface intact rather than
            // asserting a picture that cannot be drawn.
            Label::builder()
                .label("")
                .css_classes(shape_classes(shape))
                .build()
                .upcast()
        },
        |texture| {
            let picture = gtk4::Picture::for_paintable(&texture);
            picture.set_can_shrink(true);
            picture.set_css_classes(&shape_classes(shape));
            picture.upcast()
        },
    )
}

/// GTK size-request units for an `Image` node's `size` hint — the square,
/// aspect kept and fit rather than cropped, that a picture (or its
/// fallback initials) draws into. Extracted so Core's logical units are
/// asserted without a display, the way `targets::minimum_target_px` pins
/// the touch-target floor.
fn sized_square_px(size: u16) -> i32 {
    i32::from(size)
}

/// `size` present: the picture is fit inside a `size_px` square,
/// never cropped, centred in the width it is given — `ContentFit::Contain`
/// plus `can_shrink` do the fitting; `.avatar`'s CSS-driven sizing does not
/// apply here, since Core's hint overrides shape-based sizing rather than
/// adding to it.
fn sized_picture_widget(bytes: &[u8], size_px: i32) -> Widget {
    let gbytes = gtk4::glib::Bytes::from(bytes);
    gtk4::gdk::Texture::from_bytes(&gbytes).map_or_else(
        |_| sized_fallback_widget("", size_px),
        |texture| {
            let picture = gtk4::Picture::for_paintable(&texture);
            picture.set_content_fit(gtk4::ContentFit::Contain);
            picture.set_can_shrink(true);
            picture.set_size_request(size_px, size_px);
            picture.set_halign(gtk4::Align::Center);
            picture.upcast()
        },
    )
}

/// The fallback-text (or undecodable-picture) counterpart of
/// `sized_picture_widget`: Core's contract sizes the fallback to the same
/// square a picture would have drawn into.
fn sized_fallback_widget(text: &str, size_px: i32) -> Widget {
    let label = Label::builder()
        .label(text)
        .halign(gtk4::Align::Center)
        .valign(gtk4::Align::Center)
        .build();
    label.set_size_request(size_px, size_px);
    label.upcast()
}

fn image_widget(
    content: &ImageContent<'_>,
    shape: PresentationImageShape,
    target_px: i32,
    size: Option<u16>,
) -> Widget {
    match (content, size) {
        // Nothing to show shows nothing, whatever the shape or size: an
        // empty avatar carrying the node's accessibility label would
        // announce a picture that is not there.
        (ImageContent::Nothing, _) => GtkBox::new(Orientation::Horizontal, 0).upcast(),
        // A `size` hint overrides shape-based sizing outright — it is the
        // one case core's contract asks to fit rather than crop, so it
        // takes priority over the `Circle` shape's own avatar crop too.
        (ImageContent::Picture(bytes), Some(size)) => {
            sized_picture_widget(bytes, sized_square_px(size))
        }
        (ImageContent::Initials(initials), Some(size)) => {
            sized_fallback_widget(initials, sized_square_px(size))
        }
        _ if shape_is_circle(shape) => avatar_widget(content, target_px),
        (ImageContent::Picture(bytes), None) => picture_widget(bytes, shape),
        (ImageContent::Initials(initials), None) => Label::builder()
            .label(*initials)
            .css_classes(shape_classes(shape))
            .build()
            .upcast(),
    }
}

const QR_LIGHT_RGB: (f64, f64, f64) = (1.0, 1.0, 1.0);
const QR_DARK_RGB: (f64, f64, f64) = (0.0, 0.0, 0.0);
const QR_SIDE_PX: i32 = 200;

/// A placed display code's side and top-left corner inside its square, in
/// pixels. Pure so a test can assert the numbers without a `DrawingArea`
/// (vauchi/private#450, ported from macOS `QrFrameSpec`).
#[derive(Debug, Clone, Copy, PartialEq)]
struct QrFrame {
    side: f64,
    left: f64,
    top: f64,
}

/// `placement` is Core's `(size, x, y)` permille triple, taken apart from
/// its typed `QrPlacement` at the call site rather than threaded through
/// here: `QrPlacement::new` already keeps a real one inside the square, so
/// this stays defensive (and testable at its boundary values) without
/// depending on that invariant.
///
/// No placement, or a nonsensical size, is the full square. A size or
/// corner that still reached outside the square is pulled back in, so the
/// code is never drawn past its node.
fn qr_frame(placement: Option<(u16, u16, u16)>, square_side: f64) -> QrFrame {
    const FULL: f64 = 1000.0;
    let Some((size, x, y)) = placement.filter(|(size, ..)| *size > 0) else {
        return QrFrame {
            side: square_side,
            left: 0.0,
            top: 0.0,
        };
    };
    let size = f64::from(size).min(FULL);
    let room = FULL - size;
    // Multiply before dividing: 650 × 200 / 1000 is exact.
    let scaled = |permille: f64| permille * square_side / FULL;
    QrFrame {
        side: scaled(size),
        left: scaled(f64::from(x).min(room)),
        top: scaled(f64::from(y).min(room)),
    }
}

/// Core Image's `low`/`medium` naming for `qrcode`'s `EcLevel`: `Low` maps
/// to `L`; absent or an unrecognised (future `non_exhaustive`) level draws
/// at `M`, the level this shell drew before placement/error_correction
/// existed (vauchi/private#450).
fn qr_error_correction_level(level: Option<PresentationQrErrorCorrection>) -> qrcode::EcLevel {
    match level {
        Some(PresentationQrErrorCorrection::Low) => qrcode::EcLevel::L,
        _ => qrcode::EcLevel::M,
    }
}

pub(super) fn render(
    node: &PresentationNode,
    surface_id: &SurfaceId,
    on_event: &OnEvent,
    link: &EngineHandle,
    tokens: &PresentationTokens,
) -> Widget {
    match node {
        PresentationNode::Group {
            label,
            axis,
            children,
            accessibility,
            ..
        } => {
            let group = GtkBox::new(
                match axis {
                    vauchi_contract::PresentationAxis::Horizontal => Orientation::Horizontal,
                    vauchi_contract::PresentationAxis::Vertical => Orientation::Vertical,
                },
                8,
            );
            if let Some(label) = label {
                group.append(
                    &Label::builder()
                        .label(label)
                        .css_classes(["heading"])
                        .halign(gtk4::Align::Start)
                        .build(),
                );
            }
            for child in children {
                group.append(&render_node(child, surface_id, on_event, link, tokens));
            }
            apply_accessibility(&group, accessibility);
            group.upcast()
        }
        PresentationNode::List {
            label,
            rows,
            searchable,
            accessibility,
            search_placeholder,
            ..
        } => {
            let group = GtkBox::new(Orientation::Vertical, 4);
            apply_accessibility(&group, accessibility);
            if let Some(label) = label {
                group.append(
                    &Label::builder()
                        .label(label)
                        .css_classes(["heading"])
                        .halign(gtk4::Align::Start)
                        .build(),
                );
            }
            if *searchable {
                let search = gtk4::SearchEntry::new();
                if let Some(placeholder) = search_placeholder {
                    search.set_placeholder_text(Some(placeholder));
                }
                group.append(&search);
            }
            for row in rows {
                group.append(&render_row(row, surface_id, on_event, link, tokens));
            }
            group.upcast()
        }
        PresentationNode::Image {
            data,
            fallback_text,
            shape,
            size,
            activation,
            accessibility,
            ..
        } => {
            let content = image_content(data.as_deref(), fallback_text.as_deref());
            let target_px = targets::minimum_target_px(tokens);
            activation.as_ref().map_or_else(
                || {
                    let widget = image_widget(&content, *shape, target_px, *size);
                    apply_accessibility(&widget, accessibility);
                    widget
                },
                |action| {
                    let button = action_button(action, surface_id, on_event, tokens);
                    button.set_child(Some(&image_widget(&content, *shape, target_px, *size)));
                    apply_accessibility(&button, accessibility);
                    button.upcast()
                },
            )
        }
        PresentationNode::Status {
            title,
            detail,
            icon_token,
            badge,
            activation,
            accessibility,
            ..
        } => {
            let text = [Some(title.as_str()), detail.as_deref(), badge.as_deref()]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" — ");
            let pictogram = content_pictogram(icon_token.as_deref());
            activation.as_ref().map_or_else(
                || {
                    let status = Label::builder()
                        .label(&text)
                        .wrap(true)
                        .halign(gtk4::Align::Start)
                        .build();
                    let Some(pictogram) = &pictogram else {
                        apply_accessibility(&status, accessibility);
                        return status.upcast();
                    };
                    let with_icon = GtkBox::new(Orientation::Horizontal, 8);
                    with_icon.append(pictogram);
                    with_icon.append(&status);
                    apply_accessibility(&with_icon, accessibility);
                    with_icon.upcast()
                },
                |action| {
                    let button = action_button(action, surface_id, on_event, tokens);
                    match &pictogram {
                        Some(pictogram) => {
                            let content = GtkBox::new(Orientation::Horizontal, 8);
                            content.append(pictogram);
                            content.append(&Label::new(Some(&text)));
                            button.set_child(Some(&content));
                        }
                        None => button.set_label(&text),
                    }
                    apply_accessibility(&button, accessibility);
                    button.upcast()
                },
            )
        }
        PresentationNode::Qr {
            id,
            payloads,
            purpose,
            label,
            accessibility,
            placement,
            error_correction,
            capture_placeholder,
            capture_submit_label,
            ..
        } => {
            let group = GtkBox::new(Orientation::Vertical, 4);
            apply_accessibility(&group, accessibility);
            match purpose {
                PresentationQrPurpose::Display => {
                    if let Some(payload) = payloads.first() {
                        let code = render_qr(payload, *placement, *error_correction);
                        // A custom-drawn surface has no intrinsic accessible
                        // identity: unnamed and roleless, a screen reader
                        // cannot announce the QR code at all.
                        code.set_accessible_role(gtk4::AccessibleRole::Img);
                        apply_accessibility(&code, accessibility);
                        group.append(&code);
                    }
                }
                PresentationQrPurpose::Capture => {
                    group.append(&render_qr_capture(
                        id,
                        surface_id,
                        on_event,
                        QrCaptureCopy {
                            placeholder: capture_placeholder.as_deref(),
                            submit_label: capture_submit_label.as_deref(),
                            accessible_name: &accessibility.label,
                        },
                    ));
                }
            }
            if let Some(label) = label {
                group.append(&Label::new(Some(label)));
            }
            group.upcast()
        }
        PresentationNode::Divider => gtk4::Separator::new(Orientation::Horizontal).upcast(),
        _ => Label::new(None).upcast(),
    }
}

fn render_qr(
    payload: &str,
    placement: Option<QrPlacement>,
    error_correction: Option<PresentationQrErrorCorrection>,
) -> DrawingArea {
    let drawing = DrawingArea::builder()
        .width_request(QR_SIDE_PX)
        .height_request(QR_SIDE_PX)
        .halign(gtk4::Align::Center)
        .build();
    let level = qr_error_correction_level(error_correction);
    let modules = qrcode::QrCode::with_error_correction_level(payload, level)
        .ok()
        .map(|code| {
            code.to_colors()
                .chunks(code.width())
                .map(|row| {
                    row.iter()
                        .map(|color| *color == qrcode::Color::Dark)
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>()
        });
    let placement = placement.map(|p| (p.size(), p.x(), p.y()));
    drawing.set_draw_func(move |_, context, width, _height| {
        context.set_source_rgb(QR_LIGHT_RGB.0, QR_LIGHT_RGB.1, QR_LIGHT_RGB.2);
        let _ = context.paint();
        let Some(modules) = &modules else {
            return;
        };
        context.set_source_rgb(QR_DARK_RGB.0, QR_DARK_RGB.1, QR_DARK_RGB.2);
        // The node is square (width_request == height_request), so one
        // frame covers both axes.
        let frame = qr_frame(placement, f64::from(width));
        let module_side = frame.side / modules.len() as f64;
        for (y, row) in modules.iter().enumerate() {
            for (x, dark) in row.iter().enumerate() {
                if *dark {
                    context.rectangle(
                        frame.left + x as f64 * module_side,
                        frame.top + y as f64 * module_side,
                        module_side.ceil(),
                        module_side.ceil(),
                    );
                }
            }
        }
        let _ = context.fill();
    });
    drawing
}

/// The copy Core prepared for a QR capture (contract v0.97.0). Without a
/// submit label the button is an icon named by the node's accessibility
/// label, so the renderer picks no word of its own (ADR-066).
struct QrCaptureCopy<'a> {
    placeholder: Option<&'a str>,
    submit_label: Option<&'a str>,
    accessible_name: &'a str,
}

fn render_qr_capture(
    id: &BindingId,
    surface_id: &SurfaceId,
    on_event: &OnEvent,
    copy: QrCaptureCopy<'_>,
) -> GtkBox {
    let row = GtkBox::new(Orientation::Horizontal, 8);
    let entry = Entry::builder().hexpand(true).build();
    if let Some(placeholder) = copy.placeholder {
        entry.set_placeholder_text(Some(placeholder));
    }
    entry.set_widget_name(id.as_str());
    let button = match copy.submit_label {
        Some(label) => Button::with_label(label),
        None => {
            let button = Button::from_icon_name("object-select-symbolic");
            crate::core_ui::accessibility::apply_label(&button, copy.accessible_name);
            button
        }
    };
    let entry_for_submit = entry.clone();
    let id = id.clone();
    let surface = surface_id.clone();
    let callback = on_event.clone();
    button.connect_clicked(move |_| {
        let value = entry_for_submit.text().to_string();
        if !value.trim().is_empty() {
            emit_value(&surface, &id, InputValue::Text(value), &callback);
        }
    });
    row.append(&entry);
    row.append(&button);
    row
}

/// The row's avatar: Core's image bytes, or the initials it prepared for when
/// there are none. `PresentationRow` carries no `shape` — a contact row is
/// always a person, so it is always round.
fn row_leading(row: &PresentationRow) -> Option<Widget> {
    match image_content(row.image_data.as_deref(), row.fallback_text.as_deref()) {
        ImageContent::Picture(bytes) => {
            let gbytes = gtk4::glib::Bytes::from(bytes);
            gtk4::gdk::Texture::from_bytes(&gbytes).ok().map(|texture| {
                let image = gtk4::Image::from_paintable(Some(&texture));
                image.set_pixel_size(32);
                image.upcast()
            })
        }
        ImageContent::Initials(initials) => Some(
            Label::builder()
                .label(initials)
                .css_classes(["avatar"])
                .build()
                .upcast(),
        ),
        ImageContent::Nothing => None,
    }
}

/// A leading pictogram for a list row or status node, drawn as a symbolic
/// icon so it takes the surrounding text colour. Decorative: the text next to
/// it already names the mode.
fn content_pictogram(token: Option<&str>) -> Option<Widget> {
    let theme = crate::core_ui::pictograms::icon_theme()?;
    let name = content_pictogram_name(token, |name| theme.has_icon(name))?;
    Some(
        gtk4::Image::builder()
            .icon_name(name)
            .pixel_size(24)
            .accessible_role(gtk4::AccessibleRole::Presentation)
            .build()
            .upcast(),
    )
}

fn render_row(
    row: &PresentationRow,
    surface_id: &SurfaceId,
    on_event: &OnEvent,
    link: &EngineHandle,
    tokens: &PresentationTokens,
) -> Widget {
    let content = GtkBox::new(Orientation::Vertical, 2);
    content.append(
        &Label::builder()
            .label(&row.title)
            .halign(gtk4::Align::Start)
            .build(),
    );
    if let Some(subtitle) = &row.subtitle {
        content.append(
            &Label::builder()
                .label(subtitle)
                .css_classes(["dim-label"])
                .halign(gtk4::Align::Start)
                .build(),
        );
    }
    if let Some(detail) = &row.detail {
        content.append(
            &Label::builder()
                .label(detail)
                .css_classes(["dim-label"])
                .halign(gtk4::Align::Start)
                .build(),
        );
    }
    for control in &row.controls {
        content.append(&render_node(control, surface_id, on_event, link, tokens));
    }
    // Avatar and text form one accessible unit: the row's name has to cover
    // what a reader will land on, not just the text beside the picture.
    let inner = GtkBox::new(Orientation::Horizontal, 8);
    if let Some(leading) = row_leading(row).or_else(|| content_pictogram(row.icon_token.as_deref()))
    {
        inner.append(&leading);
    }
    content.set_hexpand(true);
    inner.append(&content);

    let row_box = GtkBox::new(Orientation::Horizontal, 8);
    row_box.set_size_request(-1, targets::minimum_target_px(tokens));
    row_box.add_css_class(targets::TARGET_RADIUS_CLASS);
    row_box.add_css_class("vauchi-row");
    if let Some(action) = &row.activation {
        let button = action_button(action, surface_id, on_event, tokens);
        button.set_child(Some(&inner));
        button.set_hexpand(true);
        apply_accessibility(&button, &row.accessibility);
        row_box.append(&button);
    } else {
        inner.set_hexpand(true);
        apply_accessibility(&inner, &row.accessibility);
        row_box.append(&inner);
    }
    for action in row_action_controls(row) {
        row_box.append(&action_button(action, surface_id, on_event, tokens));
    }
    row_box.upcast()
}

/// The buttons a row draws after its primary content, in Core's priority
/// order: the row's own explanation first (vauchi/private#515), then
/// whatever else it attached — matching the placement every other shell
/// gives Core's `info` action relative to `secondary_actions`.
fn row_action_controls(row: &PresentationRow) -> Vec<&ActionSpec> {
    row.info
        .iter()
        .chain(row.secondary_actions.iter())
        .collect()
}

// INLINE_TEST_REQUIRED: asserts on the private `image_content`/`shape_is_circle`
// decisions, which have no public accessor and cannot be reached through a
// widget without a display.
#[cfg(test)]
mod image_content_tests {
    use super::{ImageContent, image_content, shape_is_circle, sized_square_px};
    use vauchi_contract::PresentationImageShape;

    // @internal
    #[test]
    fn image_bytes_win_over_the_initials_core_prepared() {
        assert_eq!(
            image_content(Some(&[1, 2, 3]), Some("BS")),
            ImageContent::Picture(&[1, 2, 3])
        );
    }

    // @internal
    #[test]
    fn initials_stand_in_when_there_are_no_bytes() {
        assert_eq!(
            image_content(None, Some("BS")),
            ImageContent::Initials("BS")
        );
    }

    /// Empty is not a picture. Core sends `Some(vec![])` for an avatar that
    /// was cleared, and treating it as image data drew an empty frame where
    /// the initials belonged.
    // @internal
    #[test]
    fn empty_bytes_fall_through_to_the_initials() {
        assert_eq!(
            image_content(Some(&[]), Some("BS")),
            ImageContent::Initials("BS")
        );
    }

    // @internal
    #[test]
    fn nothing_to_show_resolves_to_nothing() {
        assert_eq!(image_content(None, None), ImageContent::Nothing);
        assert_eq!(image_content(Some(&[]), Some("")), ImageContent::Nothing);
    }

    /// `Circle` is the only shape libadwaita's avatar widget draws; every
    /// other shape, including one this build has not learned, keeps its own
    /// corners instead.
    // @internal
    #[test]
    fn only_circle_resolves_to_the_avatar_widget() {
        assert!(shape_is_circle(PresentationImageShape::Circle));
        assert!(!shape_is_circle(PresentationImageShape::Natural));
    }

    // @internal
    #[test]
    fn sized_square_px_carries_cores_logical_units_into_gtk_pixels() {
        assert_eq!(sized_square_px(88), 88);
    }
}

// INLINE_TEST_REQUIRED: a display code's placement and error-correction
// level are pure arithmetic on Core's permille fields — no widget or
// display needed to assert the numbers (vauchi/private#450, ported from
// macOS `QrFrameSpec`/`qrCorrectionLevel`, squareSide 240 rescaled to
// this shell's 200px QR_SIDE_PX).
#[cfg(test)]
mod qr_placement_tests {
    use super::{QrFrame, qr_frame};

    // @internal
    #[test]
    fn no_placement_draws_the_code_edge_to_edge() {
        assert_eq!(
            qr_frame(None, 200.0),
            QrFrame {
                side: 200.0,
                left: 0.0,
                top: 0.0
            }
        );
    }

    // @internal
    #[test]
    fn a_placed_code_is_scaled_and_offset_within_the_square() {
        assert_eq!(
            qr_frame(Some((650, 350, 175)), 200.0),
            QrFrame {
                side: 130.0,
                left: 70.0,
                top: 35.0
            }
        );
        assert_eq!(
            qr_frame(Some((800, 200, 0)), 200.0),
            QrFrame {
                side: 160.0,
                left: 40.0,
                top: 0.0
            }
        );
    }

    /// Core's own `QrPlacement` already keeps a placement inside the
    /// square (`QrPlacement::new` rejects it otherwise), so this never
    /// happens over the wire. A shell still must not draw past its node.
    // @internal
    #[test]
    fn a_placement_reaching_outside_the_square_is_pulled_back_inside() {
        assert_eq!(
            qr_frame(Some((800, 900, 5000)), 200.0),
            QrFrame {
                side: 160.0,
                left: 40.0,
                top: 40.0
            }
        );
        assert_eq!(
            qr_frame(Some((4000, 10, 10)), 200.0),
            QrFrame {
                side: 200.0,
                left: 0.0,
                top: 0.0
            }
        );
    }

    // @internal
    #[test]
    fn a_nonsensical_size_falls_back_to_the_full_square() {
        assert_eq!(
            qr_frame(Some((0, 0, 0)), 200.0),
            QrFrame {
                side: 200.0,
                left: 0.0,
                top: 0.0
            }
        );
    }
}

#[cfg(test)]
mod qr_error_correction_tests {
    use super::qr_error_correction_level;
    use vauchi_contract::PresentationQrErrorCorrection;

    // @internal
    #[test]
    fn low_maps_to_the_low_qr_ec_level() {
        assert_eq!(
            qr_error_correction_level(Some(PresentationQrErrorCorrection::Low)),
            qrcode::EcLevel::L
        );
    }

    // @internal
    #[test]
    fn absent_or_medium_maps_to_the_medium_qr_ec_level() {
        assert_eq!(qr_error_correction_level(None), qrcode::EcLevel::M);
        assert_eq!(
            qr_error_correction_level(Some(PresentationQrErrorCorrection::Medium)),
            qrcode::EcLevel::M
        );
    }
}

// INLINE_TEST_REQUIRED: the row the buttons draw from has no public
// accessor and cannot be reached through a widget without a display.
#[cfg(test)]
mod row_action_control_tests {
    use super::{PresentationRow, row_action_controls};
    use vauchi_contract::{AccessibilitySpec, ActionSpec, ActionTone, InteractionId};

    fn action(id: &str) -> ActionSpec {
        ActionSpec {
            interaction_id: InteractionId::new(id).expect("interaction id"),
            label: id.into(),
            accessibility_label: id.into(),
            icon_token: None,
            enabled: true,
            tone: ActionTone::Standard,
            shortcut: None,
        }
    }

    fn row(info: Option<ActionSpec>, secondary_actions: Vec<ActionSpec>) -> PresentationRow {
        PresentationRow {
            title: "Row".into(),
            subtitle: None,
            detail: None,
            icon_token: None,
            image_data: None,
            fallback_text: None,
            selected: false,
            enabled: true,
            activation: None,
            secondary_actions,
            info,
            controls: Vec::new(),
            accessibility: AccessibilitySpec::label("Row"),
        }
    }

    // @internal
    #[test]
    fn the_rows_own_explanation_leads_whatever_core_attached_after_it() {
        let info = action("row.info");
        let more = action("row.more");
        let subject = row(Some(info.clone()), vec![more.clone()]);

        assert_eq!(row_action_controls(&subject), vec![&info, &more]);
    }

    // @internal
    #[test]
    fn a_row_without_an_explanation_draws_only_its_secondary_actions() {
        let more = action("row.more");
        let subject = row(None, vec![more.clone()]);

        assert_eq!(row_action_controls(&subject), vec![&more]);
    }

    // @internal
    #[test]
    fn a_row_with_neither_draws_no_action_controls() {
        let subject = row(None, Vec::new());

        assert!(row_action_controls(&subject).is_empty());
    }
}
