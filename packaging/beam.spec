#
# spec file for package beam
#
# Copyright (c) 2026 Rodrigo Brito
#
# This program is free software: you can redistribute it and/or modify
# it under the terms of the GNU General Public License as published by
# the Free Software Foundation, either version 3 of the License, or
# (at your option) any later version.
#

Name:           beam
Version:        0.1.0
Release:        0
Summary:        Cliente RDP do ecossistema Lyra Enterprise Linux
License:        GPL-3.0-or-later
Group:          Productivity/Networking/Remote Desktop
URL:            https://github.com/britors/Beam
Source0:        %{name}-%{version}.tar.gz
Source1:        vendor.tar.zst

BuildRequires:  cargo
BuildRequires:  rust >= 1.85
BuildRequires:  gtk4-devel >= 4.12
BuildRequires:  libadwaita-devel >= 1.5
BuildRequires:  pkgconfig
BuildRequires:  desktop-file-utils
BuildRequires:  appstream-glib
BuildRequires:  fdupes

%description
Beam é o cliente RDP (Remote Desktop Protocol) do ecossistema Lyra Enterprise
Linux, para conexão com máquinas Windows (desktops e servidores). É um
aplicativo independente, utilizável em qualquer distribuição Linux moderna,
com integração visual e funcional prioritária ao Lyra (GNOME/Wayland).

Implementado em Rust, usando IronRDP (sem FFI para libfreerdp) e GTK4 +
libadwaita. As credenciais são armazenadas no Serviço de Segredos do sistema
(GNOME Keyring / KWallet); certificados de servidor são validados por
confiança no primeiro uso (TOFU), como o known_hosts do SSH.

%prep
# -a1 extracts Source0, then unpacks Source1 (vendor.tar.zst) on top of it; the vendor
# tarball produced by the cargo_vendor OBS service already includes .cargo/config.toml, so
# no manual step is needed to point cargo at the vendored crates.
%autosetup -a1

%build
%{cargo_build}

%install
install -Dm0755 target/release/beam %{buildroot}%{_bindir}/beam
install -Dm0644 data/org.lyraos.Beam.desktop \
    %{buildroot}%{_datadir}/applications/org.lyraos.Beam.desktop
install -Dm0644 data/org.lyraos.Beam.metainfo.xml \
    %{buildroot}%{_datadir}/metainfo/org.lyraos.Beam.metainfo.xml
install -Dm0644 data/icons/org.lyraos.Beam.svg \
    %{buildroot}%{_datadir}/icons/hicolor/scalable/apps/org.lyraos.Beam.svg
install -Dm0644 data/icons/org.lyraos.Beam-symbolic.svg \
    %{buildroot}%{_datadir}/icons/hicolor/symbolic/apps/org.lyraos.Beam-symbolic.svg

desktop-file-validate %{buildroot}%{_datadir}/applications/org.lyraos.Beam.desktop
appstream-util validate-relax --nonet \
    %{buildroot}%{_datadir}/metainfo/org.lyraos.Beam.metainfo.xml

%check
# GUI/network integration tests need a display and a real RDP server; only
# the toolkit-agnostic beam-core unit tests run during package build.
cargo test --offline -p beam-core

%post
%desktop_database_post
%icon_theme_cache_post

%postun
%desktop_database_postun
%icon_theme_cache_postun

%files
%license LICENSE
%doc README.md
%{_bindir}/beam
%{_datadir}/applications/org.lyraos.Beam.desktop
%{_datadir}/metainfo/org.lyraos.Beam.metainfo.xml
%{_datadir}/icons/hicolor/scalable/apps/org.lyraos.Beam.svg
%{_datadir}/icons/hicolor/symbolic/apps/org.lyraos.Beam-symbolic.svg

%changelog
