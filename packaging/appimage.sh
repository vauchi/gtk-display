#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Mattia Egloff <mattia.egloff@pm.me>
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Build an AppImage from the release binary.
# Expects: target/release/gvauchi exists, linuxdeploy available.

set -euo pipefail

VERSION="${1:-0.0.0}"
APPDIR="AppDir"

rm -rf "$APPDIR"
mkdir -p "${APPDIR}/usr/bin"
mkdir -p "${APPDIR}/usr/share/applications"
mkdir -p "${APPDIR}/usr/share/metainfo"
mkdir -p "${APPDIR}/usr/share/icons/hicolor/scalable/apps"

cp target/release/gvauchi "${APPDIR}/usr/bin/"
# A portable image has no stable installed path for D-Bus activation (#764).
sed 's/^DBusActivatable=true$/DBusActivatable=false/' \
  data/app.vauchi.Vauchi.desktop > "${APPDIR}/usr/share/applications/app.vauchi.Vauchi.desktop"
cp data/app.vauchi.Vauchi.metainfo.xml "${APPDIR}/usr/share/metainfo/"
cp data/app.vauchi.Vauchi.svg \
  "${APPDIR}/usr/share/icons/hicolor/scalable/apps/app.vauchi.Vauchi.svg"

export DEPLOY_GTK_VERSION=4
linuxdeploy --appdir "$APPDIR" \
  --desktop-file "${APPDIR}/usr/share/applications/app.vauchi.Vauchi.desktop" \
  --icon-file "${APPDIR}/usr/share/icons/hicolor/scalable/apps/app.vauchi.Vauchi.svg" \
  --plugin gtk \
  --output appimage

mv Vauchi*.AppImage "vauchi-gtk-${VERSION}-x86_64.AppImage" 2>/dev/null \
  || mv *.AppImage "vauchi-gtk-${VERSION}-x86_64.AppImage"
echo "Done: Built vauchi-gtk-${VERSION}-x86_64.AppImage"
