# SPDX-FileCopyrightText: 2026 Mattia Egloff <mattia.egloff@pm.me>
# SPDX-License-Identifier: GPL-3.0-or-later

Name:           vauchi-gtk
Version:        0.6.0
Release:        1%{?dist}
Summary:        Privacy-focused updatable contact cards (GTK desktop)

License:        GPL-3.0-or-later AND AGPL-3.0-or-later
URL:            https://vauchi.app
Source0:        %{name}-%{version}.tar.gz

BuildRequires:  cargo
BuildRequires:  rust
BuildRequires:  gtk4-devel
BuildRequires:  alsa-lib-devel
BuildRequires:  dbus-devel
BuildRequires:  clang-devel
BuildRequires:  fontconfig-devel
BuildRequires:  libadwaita-devel
BuildRequires:  qrencode-devel
BuildRequires:  pkgconfig

Requires:       gtk4
Requires:       libadwaita
Requires:       qrencode-libs

%description
Vauchi lets you exchange updatable contact cards in person.
End-to-end encrypted and decentralized. This package provides
the native Linux desktop client built with GTK4 and libadwaita.

%prep
%autosetup

%build
cargo build --release --features "audio,ble,camera"

%install
install -Dm 755 target/release/gvauchi \
    %{buildroot}%{_bindir}/gvauchi
install -Dm 644 data/com.vauchi.desktop.desktop \
    %{buildroot}%{_datadir}/applications/com.vauchi.desktop.desktop
install -Dm 644 data/com.vauchi.desktop.service \
    %{buildroot}%{_datadir}/dbus-1/services/com.vauchi.desktop.service
install -Dm 644 data/com.vauchi.desktop.metainfo.xml \
    %{buildroot}%{_datadir}/metainfo/com.vauchi.desktop.metainfo.xml

%files
%license LICENSE
%{_bindir}/gvauchi
%{_datadir}/applications/com.vauchi.desktop.desktop
%{_datadir}/dbus-1/services/com.vauchi.desktop.service
%{_datadir}/metainfo/com.vauchi.desktop.metainfo.xml

%changelog
* Tue Nov 03 2026 Mattia Egloff <mattia.egloff@pm.me> - 0.6.0-1
- Hardware through linux-hw's LinuxHardware; fonts from Core's
  presentation tokens; the renderer on the contract (private#1029)

* Sun Mar 29 2026 Mattia Egloff <mattia.egloff@pm.me> - 0.5.0-1
- Initial packaging of vauchi-gtk for Fedora/RHEL
