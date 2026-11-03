# SPDX-FileCopyrightText: 2026 Mattia Egloff <mattia.egloff@pm.me>
# SPDX-License-Identifier: GPL-3.0-or-later

"""Typing into a text input keeps every character (vauchi/private#1062).

Each keystroke reports the value to Core, and Core answers with the whole
surface again. The shell re-renders it and gives focus back to the input;
if that restore selects the text, the next key replaces it instead of
appending. Real key events, not EditableText inserts: only typed keys go
through the focus and selection a person sees.
"""

import gi

gi.require_version("Atspi", "2.0")
from gi.repository import Atspi  # noqa: E402

from helpers import click_button, dump_tree, find_one, wait_for_element, wait_until  # noqa: E402


def _type(text: str) -> None:
    for character in text:
        Atspi.generate_keyboard_event(0, character, Atspi.KeySynthType.STRING)


def _name_entry(app):
    return find_one(app, role="text", name="Display name input")


def _focused(app) -> bool:
    entry = _name_entry(app)
    return entry is not None and entry.get_state_set().contains(Atspi.StateType.FOCUSED)


def _focus_name(app) -> None:
    # GTK4's bridge refuses Component.grab_focus; reach the field the way a
    # keyboard user does.
    for _ in range(12):
        if _focused(app):
            return
        Atspi.generate_keyboard_event(0xFF09, None, Atspi.KeySynthType.SYM)
        wait_until(lambda: True, timeout=0.1)


def _name_text(app) -> str:
    # Looked up each time: a re-render may replace the entry's widget.
    entry = _name_entry(app)
    if entry is None:
        return ""
    return Atspi.Text.get_text(entry, 0, Atspi.Text.get_character_count(entry))


# @scenario: generic_presentation_protocol.feature :: Responsive transitions preserve interaction state
def test_each_typed_key_appends_to_the_name(gtk_app_onboarding):
    app = gtk_app_onboarding
    assert click_button(app, "Create new identity"), dump_tree(app)
    entry = wait_for_element(app, role="text", name="Display name input", timeout=5.0)
    assert entry is not None, dump_tree(app)
    _focus_name(app)
    assert _focused(app), dump_tree(app)

    for typed in ("A", "Al", "Ali", "Alic", "Alice"):
        _type(typed[-1])
        wait_until(
            lambda expected=typed: _name_text(app) == expected,
            timeout=3.0,
            message=f"after typing {typed!r} the entry holds {_name_text(app)!r}",
        )

    assert _name_text(app) == "Alice"
