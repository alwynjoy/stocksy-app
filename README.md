# Stocksy Desktop — DCF Analyzer

Stocksy Desktop is a cross-platform desktop app for multi-country DCF valuation. It fetches financials from Yahoo Finance, lets a bundled Tavily search enrich forward estimates, and runs the full valuation deterministically in Rust — WACC, FCF, terminal value, scenarios, sensitivity and confidence — stored locally in SQLite.

Design docs live at `docs/design/` when you need them.

## Build Prerequisites

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

## Download & Install

### macOS

1. **Download:** [`Stocksy Desktop_0.1.0_aarch64.dmg`](https://github.com/alwynjoy/stocksy-app/releases) from [Releases](https://github.com/alwynjoy/stocksy-app/releases) (Apple Silicon M1–M4).
2. **Install:** Open the `.dmg` and drag `Stocksy Desktop.app` to `Applications`.
3. **First launch:** `Right-click` → `Open` → `Open` (or `System Settings → Privacy & Security → Open Anyway`).

### Windows

1. **Download:** [`Stocksy Desktop_0.1.0_x64-setup.exe`](https://github.com/alwynjoy/stocksy-app/releases) from [Releases](https://github.com/alwynjoy/stocksy-app/releases) (Windows 10/11 64-bit).
2. **Install:** Run `Stocksy Desktop_0.1.0_x64-setup.exe` and follow the setup wizard.
3. **First launch:** If Windows SmartScreen appears ("Windows protected your PC"), click **More info** → **Run anyway**.

## Quick Setup

1. **Add AI provider** — `Settings → LLM Provider` → pick any one (Anthropic / DeepSeek / OpenAI / Kimi / Custom) → paste API key → `Fetch Models` → choose model → `Save`.
2. **Add Tavily** — register free at Tavily → `Settings → AI Tools` → paste Tavily key → `Save`. 1000 credits free ≈ 150 reports.
3. **Analyze** — `New Analysis` → pick market → search ticker → `Analyze`.

That's it — Help is a 4-tab guide and Settings holds 7 tabs including Queue & Parallelism; everything else is in the design docs.
