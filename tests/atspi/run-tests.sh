#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Mattia Egloff <mattia.egloff@pm.me>
# SPDX-License-Identifier: GPL-3.0-or-later

# Run the renderer's AT-SPI tests under Xvfb with D-Bus + AT-SPI bus.
#
# Usage:
#   ./run-tests.sh                    # run all tests
#   ./run-tests.sh test_fixture_a11y.py  # run specific test file
#   ./run-tests.sh -k "test_app"      # pytest filter

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Force GTK4's Cairo (software) renderer so AT-SPI tests are deterministic
# and never depend on a GPU. Headless CI (Xvfb) has no DRI3 device: the GL
# renderer's "DRI3 error: Could not get DRI3 device" fallback both spams
# stderr and delays first paint past the AT-SPI appear-timeout, flaking
# test:a11y red. Cairo has no GL/EGL/DRI3 path.
export GSK_RENDERER=cairo
export LIBGL_ALWAYS_SOFTWARE=1

# If already inside a display session with AT-SPI, run directly
if [ -n "${DISPLAY:-}" ] || [ -n "${WAYLAND_DISPLAY:-}" ]; then
    exec python3 -m pytest "$SCRIPT_DIR" "$@" -v
fi

# Otherwise, run under Xvfb with a fresh D-Bus session and AT-SPI.
# XDG_CURRENT_DESKTOP=none prevents xdg-desktop-portal from activating
# compositor-specific portals (e.g., hyprland) that crash under Xvfb.

# Resolve the AT-SPI helper binaries: Debian ships them under /usr/libexec,
# Arch (and others) under /usr/lib. Hardcoding /usr/lib silently left the
# registry unlaunched on the Debian CI runner — the app then never appears on
# the AT-SPI tree and test:a11y fails every retry. Search both, then PATH.
find_atspi() {
    local name="$1" dir
    for dir in /usr/libexec /usr/lib/at-spi2-core /usr/lib; do
        if [ -x "$dir/$name" ]; then printf '%s\n' "$dir/$name"; return 0; fi
    done
    command -v "$name" 2>/dev/null && return 0
    return 1
}
AT_SPI_BUS_LAUNCHER="$(find_atspi at-spi-bus-launcher)" \
    || { echo "ERROR: at-spi-bus-launcher not found (install at-spi2-core)" >&2; exit 1; }
AT_SPI_REGISTRYD="$(find_atspi at-spi2-registryd)" \
    || { echo "ERROR: at-spi2-registryd not found (install at-spi2-core)" >&2; exit 1; }
export AT_SPI_BUS_LAUNCHER AT_SPI_REGISTRYD

PYTEST_STATUS_FILE=$(mktemp)
export PYTEST_STATUS_FILE
trap 'rm -f "$PYTEST_STATUS_FILE"' EXIT INT TERM

set +e
# -a picks a free server number. Without it every invocation claims the
# default :99, and test:a11y and test:snapshots — which both `needs:
# build:app` and share one self-hosted shell runner — start together and
# tear down each other's display mid-suite.
env XDG_CURRENT_DESKTOP=none \
    xvfb-run -a -s '-screen 0 1280x720x24' \
    dbus-run-session -- bash -c "
        set -euo pipefail

        \"\$AT_SPI_BUS_LAUNCHER\" &
        \"\$AT_SPI_REGISTRYD\" &

        # Poll for AT-SPI readiness instead of fixed sleeps. The previous
        # 0.5s sleeps were sufficient on most runners but caused 'app did
        # not appear in AT-SPI tree' flakes on loaded self-hosted runners
        # where registryd took > 1s to accept connections.
        python3 - <<'PY'
import sys, time
try:
    import gi
    gi.require_version('Atspi', '2.0')
    from gi.repository import Atspi
except Exception as exc:
    print(f'WARNING: cannot import Atspi for readiness probe: {exc}', file=sys.stderr)
    sys.exit(0)

deadline = time.monotonic() + 10.0
while time.monotonic() < deadline:
    try:
        desktop = Atspi.get_desktop(0)
        if desktop is not None and desktop.get_child_count() >= 0:
            print('AT-SPI registry ready')
            sys.exit(0)
    except Exception:
        pass
    time.sleep(0.1)
print('ERROR: AT-SPI registry did not become ready within 10s', file=sys.stderr)
sys.exit(1)
PY

        cd \"$SCRIPT_DIR\"
        set +e
        python3 -m pytest . \"\$@\" -v
        pytest_status=\$?
        set -e
        printf '%s\\n' \"\$pytest_status\" > \"\$PYTEST_STATUS_FILE\"
        exit \"\$pytest_status\"
    " _ "$@"
wrapper_status=$?
set -e

if [ -s "$PYTEST_STATUS_FILE" ]; then
    read -r pytest_status < "$PYTEST_STATUS_FILE"
    exit "$pytest_status"
fi
exit "$wrapper_status"
