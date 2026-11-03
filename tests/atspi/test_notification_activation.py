# SPDX-FileCopyrightText: 2026 Mattia Egloff <mattia.egloff@pm.me>
# SPDX-License-Identifier: GPL-3.0-or-later

"""Cold-start notification actions must reach Core through the real application."""

import os
from pathlib import Path
import signal
import shutil
import subprocess
import sys
import tempfile

import pytest


def accessibility_bus_id():
    import gi

    gi.require_version("Gio", "2.0")
    from gi.repository import Gio, GLib

    session = Gio.bus_get_sync(Gio.BusType.SESSION, None)
    address = session.call_sync(
        "org.a11y.Bus", "/org/a11y/bus", "org.a11y.Bus", "GetAddress",
        None, GLib.VariantType.new("(s)"), Gio.DBusCallFlags.NONE, 5000, None,
    ).unpack()[0]
    connection = Gio.DBusConnection.new_for_address_sync(
        address, Gio.DBusConnectionFlags.AUTHENTICATION_CLIENT
        | Gio.DBusConnectionFlags.MESSAGE_BUS_CONNECTION, None, None,
    )
    try:
        return connection.call_sync(
            "org.freedesktop.DBus", "/org/freedesktop/DBus",
            "org.freedesktop.DBus", "GetId", None,
            GLib.VariantType.new("(s)"), Gio.DBusCallFlags.NONE, 5000, None,
        ).unpack()[0]
    finally:
        connection.close_sync(None)


@pytest.fixture
def private_runtime_dir():
    # Unix sockets must fit sun_path even when CI's TMPDIR is deeply nested.
    with tempfile.TemporaryDirectory(prefix="vauchi-a11y-", dir="/tmp") as directory:
        yield directory


def test_notification_action_cold_starts_and_delivers_target(
    gtk_binary, tmp_path, private_runtime_dir,
):
    parent_bus_id = accessibility_bus_id()
    repo = Path(__file__).resolve().parents[2]
    # Keep the long-runner-path regression active on short local checkouts too.
    app_data_dir = tmp_path / ("long-runner-path-" * 8)
    service_dir = app_data_dir / "dbus-1" / "services"
    service_dir.mkdir(parents=True)
    service = (repo / "data/app.vauchi.Vauchi.service").read_text()
    # Relocate the installed executable into this test's build directory.
    service = service.replace("/usr/bin/gvauchi", str(Path(gtk_binary).resolve()))
    (service_dir / "app.vauchi.Vauchi.service").write_text(service)
    # Private-bus tests must not activate the user's keyring or desktop portal.
    for name in ("org.freedesktop.secrets", "org.freedesktop.portal.Desktop"):
        (service_dir / f"{name}.service").write_text(
            f"[D-BUS Service]\nName={name}\nExec=/usr/bin/false\n"
        )
    env = dict(os.environ)
    env.update(
        XDG_RUNTIME_DIR=private_runtime_dir,
        XDG_DATA_HOME=str(app_data_dir),
        XDG_CONFIG_HOME=str(app_data_dir / "config"),
        XDG_CACHE_HOME=str(app_data_dir / "cache"),
        XDG_CURRENT_DESKTOP="none",
        GTK_A11Y="atspi",
        GSK_RENDERER="cairo",
        LC_ALL="C.UTF-8",
    )
    for name in (
        "VAUCHI_TEST_NON_UNIQUE", "VAUCHI_TEST_SEED", "AT_SPI_BUS_ADDRESS",
        "DISPLAY", "WAYLAND_DISPLAY",
    ):
        env.pop(name, None)
    # AT-SPI also publishes sockets and X11 properties outside D-Bus. Sharing
    # the outer display/runtime lets child teardown strand later suite apps.
    result = subprocess.run(
        ["xvfb-run", "-a", "dbus-run-session", "--", sys.executable, __file__],
        env=env,
        capture_output=True,
        text=True,
        timeout=45,
        check=False,
    )
    assert result.returncode == 0, result.stdout + result.stderr
    assert "cold-start target reached Core" in result.stdout
    assert accessibility_bus_id() == parent_bus_id


def check_cold_start():
    import gi

    gi.require_version("Atspi", "2.0")
    from gi.repository import Atspi, Gio, GLib

    app_id = "app.vauchi.Vauchi"
    expected = "Contact 'notification-cold-start-764' was not found."
    bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)

    def bus_call(method, argument):
        return bus.call_sync(
            "org.freedesktop.DBus", "/org/freedesktop/DBus",
            "org.freedesktop.DBus", method, GLib.Variant("(s)", (argument,)),
            None, Gio.DBusCallFlags.NONE, 10_000, None,
        ).unpack()[0]

    assert bus_call("NameHasOwner", app_id) is False
    registry = next(
        (path for directory in ("/usr/libexec", "/usr/lib/at-spi2-core", "/usr/lib")
         if (path := Path(directory) / "at-spi2-registryd").is_file()),
        shutil.which("at-spi2-registryd"),
    )
    assert registry is not None, "at-spi2-registryd is required"
    # Some distros use dbus-broker for the a11y bus, whose service activation
    # needs a user systemd session. Start the registry explicitly in isolation.
    registry_process = subprocess.Popen([str(registry)])
    Atspi.init()
    desktop = Atspi.get_desktop(0)
    assert desktop is not None
    loop = GLib.MainLoop()
    found = []
    errors = []
    process_ids = []

    def names(node, depth=0):
        if node is None or depth > 20:
            return
        yield node.get_name()
        for index in range(node.get_child_count()):
            yield from names(node.get_child_at_index(index), depth + 1)

    def inspect(*_):
        for index in range(desktop.get_child_count()):
            app = desktop.get_child_at_index(index)
            if app and app.get_name() == "gvauchi":
                if expected in list(names(app)):
                    found.append(expected)
                    loop.quit()

    listener = Atspi.EventListener.new(inspect)
    for event in ("object:children-changed", "window:create", "object:property-change:accessible-name"):
        assert listener.register(event)

    def activated(connection, result):
        try:
            assert connection.call_finish(result).unpack() == ()
            process_ids.append(bus_call("GetConnectionUnixProcessID", app_id))
            inspect()
        except GLib.Error as error:
            errors.append(str(error))
            loop.quit()

    def deadline():
        errors.append("notification target did not reach the live AT-SPI tree")
        loop.quit()
        return GLib.SOURCE_REMOVE

    timeout = GLib.timeout_add_seconds(30, deadline)
    bus.call(
        app_id, "/com/vauchi/desktop", "org.freedesktop.Application",
        "ActivateAction",
        GLib.Variant("(sava{sv})", (
            "open-notification",
            [GLib.Variant("s", "vauchi://contact/notification-cold-start-764")], {},
        )),
        None, Gio.DBusCallFlags.NONE, 25_000, None, activated,
    )
    try:
        loop.run()
        assert errors == [], errors
        assert found == [expected]
        print("cold-start target reached Core")
    finally:
        if not errors:
            GLib.source_remove(timeout)
        for event in ("object:children-changed", "window:create", "object:property-change:accessible-name"):
            listener.deregister(event)
        for pid in process_ids:
            try:
                os.kill(pid, signal.SIGTERM)
            except ProcessLookupError:
                pass
        registry_process.terminate()
        registry_process.wait(timeout=5)


if __name__ == "__main__":
    check_cold_start()
