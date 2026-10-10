Name:           freemon
Version:        0.1.0
Release:        1%{?dist}
Summary:        Free System Monitor for KDE Plasma (Rust + Slint)
License:        GPL-3.0-or-later
URL:            https://github.com/yourname/freemon
Source0:        %{name}-%{version}.tar.gz

BuildRequires:  cargo
BuildRequires:  rustc
BuildRequires:  pkgconfig(fontconfig)
BuildRequires:  gcc

%description
Modern, fast, secure system monitor for KDE Plasma.
Processes, performance graphs, systemd services, autostart,
Flatpak apps, cleaner, logs — built with Rust + Slint.

%prep
%autosetup

%build
export RUSTFLAGS="-L %{_libdir}"
cargo build --release --offline 2>/dev/null || cargo build --release
# NOTE: first-time builds need network for crates.io unless vendored.

%install
install -Dm755 target/release/freemon %{buildroot}%{_bindir}/freemon
install -Dm644 LICENSE %{buildroot}%{_datadir}/licenses/%{name}/LICENSE

%files
%{_bindir}/freemon
%{_datadir}/licenses/%{name}/LICENSE

%changelog
* Sat Oct 10 2026 FirstPrinciples-Sun - 0.1.0-1
- Initial package: glassmorphism UI, 10 tabs, graphs everywhere
