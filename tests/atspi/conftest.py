# SPDX-FileCopyrightText: 2026 Mattia Egloff <mattia.egloff@pm.me>
# SPDX-License-Identifier: GPL-3.0-or-later

"""Pytest fixtures for AT-SPI tests of the GTK renderer.

Each test renders contract surfaces through `render_fixture`; the app's
own tests (gvauchi launched with Core) live in linux-app (ADR-078).
"""

import json
import os
import subprocess
import time

import gi
import pytest

gi.require_version("Atspi", "2.0")
from gi.repository import Atspi  # noqa: E402


@pytest.fixture
def render_surface(tmp_path):
    """Render each supplied contract in its own process through the real renderer."""
    from surface_probe import find_app_by_anchor

    binary = _find_binary("render_fixture")
    assert binary is not None, "render_fixture must be built before native tests"
    processes = []

    def render(surface):
        path = tmp_path / f"surface-{len(processes)}.json"
        path.write_text(json.dumps(surface))
        log_path = path.with_suffix(".log")
        with log_path.open("wb") as log:
            proc = subprocess.Popen(
                [binary, str(path), "--keep-open"],
                env={**os.environ, "GTK_A11Y": "atspi"},
                stdout=log, stderr=log,
            )
        processes.append(proc)
        app = find_app_by_anchor(proc.pid)
        assert app is not None, log_path.read_text()
        return app

    yield render

    for proc in processes:
        proc.terminate()
        try:
            proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.wait(timeout=5)


def _wait_for_atspi_ready(timeout: float = 10.0) -> bool:
    """Poll AT-SPI registry until it responds to a desktop query.

    Returns True once `Atspi.get_desktop(0)` yields a desktop whose
    child count can be read without raising. The loaded self-hosted
    runner occasionally needs several seconds before the a11y bus +
    registryd accept connections; launching a renderer before that window
    causes the GTK a11y bridge to give up and the app never appears in
    the tree (observed as the 2/20 test:a11y flake on 2026-04-22).
    """
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        try:
            desktop = Atspi.get_desktop(0)
            if desktop is not None:
                _ = desktop.get_child_count()
                return True
        except Exception:
            pass
        time.sleep(0.1)
    return False


def _find_binary(name):
    """Locate a binary this repository builds."""
    script_dir = os.path.dirname(os.path.abspath(__file__))
    repo_dir = os.path.dirname(os.path.dirname(script_dir))
    workspace = os.path.dirname(repo_dir)
    for base in [repo_dir, workspace]:
        for profile in ["release", "debug"]:
            path = os.path.join(base, "target", profile, name)
            if os.path.isfile(path) and os.access(path, os.X_OK):
                return path
    return None
