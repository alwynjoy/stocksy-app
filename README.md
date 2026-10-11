# Stocksy Desktop — DCF Analyzer

Stocksy Desktop is a cross-platform desktop app for multi-country DCF valuation. It fetches financials from Yahoo Finance, lets a bundled Tavily search enrich forward estimates, and runs the full valuation deterministically in Rust — WACC, FCF, terminal value, scenarios, sensitivity and confidence — stored locally in SQLite.

Design docs live at `docs/design/` when you need them.

## Download & Install (macOS)

No compilation, no `python3`, no source download needed — the `dmg` is self-contained (bundled `cpython-3.11.10` + `yfinance`).

**1) Download:** [Releases → `Stocksy Desktop_0.1.0_aarch64.dmg`](https://github.com/alwynjoy/stocksy-app/releases) (Apple Silicon M1–M4; Intel `x86_64`/`universal` not in this release).

**2) Install:** Open the `dmg` → drag `Stocksy Desktop.app` → `Applications` → eject the `dmg`.

**3) First launch (ad-hoc, not notarized yet):** `Right-click` → `Open` → `Open`, or `System Settings → Privacy & Security → Open Anyway`. Terminal alternative:

```bash
xattr -d com.apple.quarantine /Applications/Stocksy\ Desktop.app
open /Applications/Stocksy\ Desktop.app
```

Gatekeeper on Sequoia/Sonoma blocks unsigned `dmg`s — `Right-click → Open` bypasses it once. After that, launch from `Launchpad/Applications` normally.

**What’s inside:** `Tauri 2.12` app + `Resources/python/bin/python3` (portable 3.11.10) + `site-packages` (`yfinance 1.4.1` 26 pinned) + `Resources/py-scripts/extractor_cli.py`. No host `python3`/`pip` needed; updates are via `Releases` (no auto-updater — `F8.5` removed).

> **Build locally:** `bash scripts/setup-portable-python.sh && npm ci && npm run tauri build` → `src-tauri/target/release/bundle/dmg/*.dmg` + `.../macos/*.app`.

## Prerequisites

Tauri CLI is per-project via `npm` — no global `cargo install tauri-cli`.

### 1) Install Node + npm (once per machine)

```bash
# macOS
brew install node
node -v && npm -v   # ≥20.9 (22 LTS recommended; 26.x works) · npm ≥10

# Windows 10/11
winget install OpenJS.NodeJS.LTS
node -v && npm -v

# Linux — Ubuntu/Debian (or use fnm/nvm)
sudo apt update && sudo apt install nodejs npm
```

### 2) Install the frontend stack (once per clone)

```bash
npm ci            # or npm install — pulls the 25 direct packages below
npm run typecheck # tsc --noEmit (strict)
npm run build     # vite → ../dist  (Tauri beforeBuildCommand)
```

| What `npm ci` pulls | Purpose |
|---|---|
| React 18.3 + React Router 6.28 | UI + routing |
| Vite 5.4 + @vitejs/plugin-react 4.3 | dev server :1420 strictPort, HMR |
| TypeScript 5.5 + Tailwind 3.4 + PostCSS/Autoprefixer | strict `tsconfig`, `src/**/*.{ts,tsx}` |
| recharts 3.5 · lucide-react · react-markdown/remark-gfm · @react-pdf/renderer | charts · icons · Help/report markdown · PDF export |
| @tauri-apps/api · plugin-dialog/fs · cli 2.x | Tauri bridge |

### 3) Install Rust + OS webview

#### macOS

```bash
rustup update
rustc --version   # 1.98.1
rustup target add aarch64-apple-darwin
rustup target add x86_64-apple-darwin   # Intel / universal
xcode-select --install   # Command Line Tools — enough for dev; full Xcode only for signed dmg/notarize
```

#### Windows 10/11

```bash
rustup update   # MSVC toolchain
# Visual Studio 2022 — Desktop development with C++ + Windows 10/11 SDK (provides link.exe)
# WebView2 Evergreen Runtime — Evergreen Standalone Installer from Microsoft
# WiX Toolset v3.11.2 — add bin to PATH (for msi when bundle targets = "all")
rustup target add x86_64-pc-windows-msvc
```

#### Linux

Tauri v2 uses WebKitGTK **4.1** (`libsoup3`), not legacy 4.0.

```bash
# Ubuntu / Debian
sudo apt update
sudo apt install libwebkit2gtk-4.1-dev libjavascriptcoregtk-4.1-dev libgtk-3-dev \
  libayatana-appindicator3-dev librsvg2-dev libssl-dev libsoup-3.0-dev \
  build-essential curl wget file pkg-config libudev-dev
rustup target add x86_64-unknown-linux-gnu
```

```bash
# Fedora
sudo dnf install webkit2gtk4.1-devel gtk3-devel libappindicator-gtk3-devel \
  librsvg2-devel openssl-devel libsoup3-devel curl wget file pkg-config systemd-devel
```

```bash
# Arch
sudo pacman -S webkit2gtk-4.1 gtk3 libappindicator-gtk3 librsvg libsoup3 \
  curl wget file pkgconf
```

## Quick Start

```bash
npm install
npm run typecheck
npm run build
npm run tauri dev      # http://localhost:1420
cargo test             # 76 passed
cargo check
```

That's it — Help is a 4-tab guide and Settings holds 7 tabs including Queue & Parallelism; everything else is in the design docs.
