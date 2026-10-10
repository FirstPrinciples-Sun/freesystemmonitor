# ❄ FreeMon — Free System Monitor for KDE Plasma

Modern, fast, secure system monitor for Linux (KDE Plasma first).
Built with **Rust + Slint** (native, no Electron, no heavy runtime).

![status](https://img.shields.io/badge/status-v0.1_MVP-blue)
![license](https://img.shields.io/badge/license-GPL--3.0--or--later-green)

## ✨ Features (v0.1 + graphs everywhere)

| Tab | What it does |
|-----|--------------|
| ◉ Overview | CPU / Memory cards + 60s history graphs for **CPU, Memory, Swap, Net ↓, Net ↑** + System / Storage / Power cards |
| ☰ Processes | Live CPU sparkline, search, sort by CPU, **per-process Disk I/O rate** (like Windows Task Manager), Stop / Kill |
| 📈 Performance | CPU+Memory mini graphs, per-core live bars |
| ⚙ Services | Active/total usage bar, start / stop / restart |
| 🚀 Startup | Enabled/total bar, safe user-local toggles |
| 📦 Apps | Per-app size bars (relative to largest), Flatpak uninstall |
| 🧹 Cleaner | Reclaimable-space bar chart (cache / thumbnails / trash / journal) + safe clean |
| 📜 Logs | Error/warning severity bars over last journal lines + tail viewer |
| 🎨 Theme | 5 accent gradients (Breeze/Mint/Sunset/Grape/Gold), refresh 1s/2s/5s, auto-saved |

## 🎨 Design

- Breeze-inspired: dark sidebar `#31363b`, light cards, rounded 8px, soft shadows
- Big readable numbers, 1-click actions, Thai-friendly fonts via fontconfig
- English UI first, Thai (i18n) planned for v0.2

## 🔒 Security model

- **Runs as your user, no root daemon.** Monitoring is read-only (`/proc`, `/sys`, `sysinfo`).
- Privileged actions go through existing system tools, never custom setuid:
  - `kill` only sends SIGTERM/SIGKILL via `sysinfo` (fails safely on other users' processes)
  - `systemctl --user` first; system-wide actions defer to `systemctl` + polkit
  - Autostart writes **only** to `~/.config/autostart` (system files are copied, never overwritten)
  - Cleaner touches only your `~/.cache`, trash, thumbnails; journal vacuum reports "need root" instead of escalating
- No network, no telemetry, no analytics. No shell-string interpolation — all commands use argv arrays.

## 🚀 Run (Fedora / Arch, no sudo needed)

```bash
# one-time: rust toolchain (already done on dev machine)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Slint needs fontconfig at build time. If you have no sudo, use the stub:
mkdir -p ~/.local/lib/pkgconfig
printf 'prefix=/usr\nexec_prefix=${prefix}\nlibdir=/usr/lib64\nincludedir=${prefix}/include\n\nName: fontconfig\nDescription: stub\nVersion: 2.15.0\nLibs: -L${libdir} -lfontconfig\nCflags:\n' \
  > ~/.local/lib/pkgconfig/fontconfig.pc

export PKG_CONFIG_PATH="$HOME/.local/lib/pkgconfig"
export RUSTFLAGS="-L $HOME/.local/lib"   # finds our user-local libfontconfig.so symlink
cargo run --release
```

Fedora COPR `.spec` and Arch `PKGBUILD` live in `packaging/`.

## 🗺 Roadmap

- v0.2: Thai/English i18n, dark-mode follow Plasma, GPU (NVML) + sensors graphs, disk-per-mount view
- v0.3: Flatpak packaging, Flathub, polkit helper for safe root actions, process tree + renice UI
- v1.0: Charts history, alerts, autostart delay editor, snap/rpm support

## 🤝 Contributing

PRs welcome. `cargo fmt && cargo clippy` before push. UI lives in `ui/appwindow.slint`, backend in `src/backend/`.

## 📄 License

GPL-3.0-or-later. See `LICENSE`.
