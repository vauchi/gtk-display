# SPDX-FileCopyrightText: 2026 Mattia Egloff <mattia.egloff@pm.me>
# SPDX-License-Identifier: GPL-3.0-or-later

"""Native accessibility contracts with explicit, isolated presentation states.

Feature: accessibility.feature. Core projects settings/checkboxes to Toggle,
info panels to Group, field lists to List, banners to actionable Status,
and inline confirmation to Confirmation. Test those primitives directly.
"""

import pytest
from gi.repository import Atspi

from helpers import wait_until
from surface_probe import dump, find_named, names, surface_with, text_contents


@pytest.fixture(autouse=True)
def explicit_text_ranges(monkeypatch):
    read_text = Atspi.Text.get_text

    def bounded_read(widget, start, end):
        # GTK 4.18.6 clamps a negative GtkEditable end offset to zero (#270).
        assert 0 <= start <= end, "AT-SPI reads must use explicit character bounds"
        return read_text(widget, start, end)

    monkeypatch.setattr(Atspi.Text, "get_text", bounded_read)



def named(app, label):
    widget = find_named(app, label)
    assert widget is not None, f"Missing {label!r}\n{dump(app)}"
    return widget


@pytest.mark.parametrize("value", [False, True])
@pytest.mark.parametrize("enabled", [False, True])
def test_toggle_exposes_exact_label_checked_and_enabled_state(render_surface, value, enabled):
    app = render_surface(surface_with("Toggle", value=value, enabled=enabled))
    toggle = named(app, "A11Y_Toggle")
    assert toggle.get_role_name() == "check box"
    assert toggle.get_description() == "DESC_Toggle"
    assert toggle.get_state_set().contains(Atspi.StateType.CHECKED) == value
    assert toggle.get_state_set().contains(Atspi.StateType.SENSITIVE) == enabled


@pytest.mark.parametrize("original, edited", [
    ("Original text", "Edited text"),
    ("Zoë 🔑", "改訂 🐈"),
])
def test_editable_input_exposes_label_value_and_native_editing(render_surface, original, edited):
    app = render_surface(surface_with("Input", value=original))
    entry = named(app, "A11Y_Input")
    assert entry.get_role_name() == "text"
    wait_until(lambda: text_contents(entry) == original, message="Initial input text was not published")
    assert Atspi.Text.get_character_count(entry) == len(original)
    assert text_contents(entry) == original
    assert entry.get_editable_text_iface().set_text_contents(edited)
    wait_until(lambda: text_contents(entry) == edited, message="Edited input text was not published")
    assert entry.get_name() == "A11Y_Input"
    assert Atspi.Text.get_character_count(entry) == len(edited)
    assert text_contents(entry) == edited


def test_info_group_exposes_title_and_item_description(render_surface):
    item = surface_with("Status")["nodes"][0]
    app = render_surface(surface_with("Group", children=[item]))
    group = named(app, "A11Y_Group")
    assert group.get_description() == "DESC_Group"
    assert named(group, "A11Y_Status").get_description() == "DESC_Status"


def test_field_list_exposes_label_and_its_rows(render_surface):
    app = render_surface(surface_with("List"))
    rows = named(app, "A11Y_List")
    assert rows.get_description() == "DESC_List"
    assert {"A11Y_Row", "A11Y_RowAvatar", "A11Y_RowInitials"} <= set(names(rows))
    assert "vis_detail" in names(rows)


@pytest.mark.parametrize("action", ["Confirm", "Cancel"])
def test_confirmation_exposes_named_native_actions(render_surface, action):
    app = render_surface(surface_with("Confirmation"))
    confirmation = named(app, "A11Y_Confirmation")
    assert "vis_warning" in names(confirmation)
    button = named(confirmation, f"A11Y_ACTION_{action}")
    assert button.get_role_name() in ("button", "push button")
    assert button.get_action_iface().get_n_actions() == 1
    assert button.get_state_set().contains(Atspi.StateType.SENSITIVE)


def banner_surface():
    action = surface_with("Confirmation")["nodes"][0]["Confirmation"]["confirm"]
    action["accessibility_label"] = "A11Y_Status"
    return surface_with("Status", activation=action, tone="accent")


def test_banner_has_core_accessibility_copy(render_surface):
    app = render_surface(banner_surface())
    banner = named(app, "A11Y_Status")
    assert banner.get_description() == "DESC_Status"
    assert "vis_status" in names(banner)


def test_banner_exposes_named_native_action(render_surface):
    app = render_surface(banner_surface())
    banner = named(app, "A11Y_Status")
    assert banner.get_role_name() in ("button", "push button")
    assert banner.get_action_iface().get_n_actions() == 1
    assert banner.get_state_set().contains(Atspi.StateType.SENSITIVE)


def test_status_exposes_title_and_detail(render_surface):
    app = render_surface(surface_with("Status", detail="Status detail"))
    status = named(app, "A11Y_Status")
    assert status.get_description() == "DESC_Status"
    assert text_contents(status) == "vis_status — Status detail"



def placeholder_of(widget):
    attributes = widget.get_attributes() or {}
    return attributes.get("placeholder-text")


def entries(app):
    from surface_probe import nodes_with_role
    return nodes_with_role(app, {"text", "entry"})


# @scenario: generic_presentation_protocol.feature :: Every shell renders the same prepared presentation
def test_a_qr_capture_shows_the_copy_core_sends(render_surface):
    app = render_surface(surface_with(
        "Qr",
        purpose="capture",
        payloads=[],
        capture_placeholder="CORE_Paste",
        capture_submit_label="CORE_Submit",
    ))
    assert named(app, "CORE_Submit").get_role_name() == "button"
    wait_until(
        lambda: any(placeholder_of(entry) == "CORE_Paste" for entry in entries(app)),
        message=f"no entry shows Core's placeholder\n{dump(app)}",
    )


# @scenario: generic_presentation_protocol.feature :: Every shell renders the same prepared presentation
def test_a_searchable_list_shows_the_placeholder_core_sends(render_surface):
    app = render_surface(surface_with(
        "List",
        searchable=True,
        search_placeholder="CORE_Search",
    ))
    wait_until(
        lambda: any(placeholder_of(entry) == "CORE_Search" for entry in entries(app)),
        message=f"no search field shows Core's placeholder\n{dump(app)}",
    )
