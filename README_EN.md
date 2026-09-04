# Codex++ (Personal Fork)

<p align="center">
  <img src="docs/images/codex-plus-plus.png" alt="Codex++ icon" width="160">
</p>

<p align="center">
  <a href="README.md">中文</a> | English
</p>

<p align="center">
  <img alt="License" src="https://img.shields.io/github/license/payAgain/CodexPlusPlus">
  <img alt="Rust" src="https://img.shields.io/badge/rust-1.85%2B-orange">
  <img alt="Tauri" src="https://img.shields.io/badge/tauri-2.x-24C8DB">
</p>

Codex++ is an external launcher and manager for the OpenAI Codex / ChatGPT desktop app. It uses the Chromium DevTools Protocol and a local helper for provider switching, protocol conversion, session management, and UI enhancements without modifying the official app's `app.asar` or installation files.

This repository is a personal fork of [BigPizzaV3/CodexPlusPlus](https://github.com/BigPizzaV3/CodexPlusPlus). It trims the upstream feature set to match personal usage: sponsor surfaces, Dream Skin management, Grok configuration, Zed Remote, and Upstream worktree have been removed, while the core provider, model context, session, enhancement, and script capabilities remain. The fork changes cover about 80 files with roughly 29,000 lines removed.

## Fork Changes

| Change | Details |
| --- | --- |
| Removed sponsor surfaces | Removed the `ads` module, frontend sponsor sections, related copy, and image assets |
| Removed Dream Skin management | Removed skin UI, runtime/community/marketplace/packaging modules, bundled and third-party skin assets, and cleared the Tauri asset protocol skin directory scope |
| Removed Grok configuration | Removed the `grok_config` module and frontend Grok manager UI |
| Removed Zed Remote | Removed remote project discovery, opening, and related injection logic |
| Removed Upstream worktree | Removed worktree creation, remote management, and related injection logic |
| Simplified shared paths | Trimmed `renderer-inject.js`, `styles.css`, Tauri commands/lib, launcher, and tests without affecting remaining features |

## Branches and Maintenance

- `main`: mirrors upstream `main` and only receives synchronization updates from [BigPizzaV3/CodexPlusPlus](https://github.com/BigPizzaV3/CodexPlusPlus). Personal changes are never merged here.
- `codex/remove-extra-features`: the personal maintenance branch and the GitHub default branch for this repository. All fork changes live here.

Sync with upstream:

```bash
git remote add upstream https://github.com/BigPizzaV3/CodexPlusPlus.git
git fetch upstream
git checkout main
git merge --ff-only upstream/main
git checkout codex/remove-extra-features
git rebase main
```

## Installers

This fork does not publish standalone installers. Use the [upstream Releases](https://github.com/BigPizzaV3/CodexPlusPlus/releases) or build locally as described in the Development section.

## Current Features

| Area | Capabilities |
| --- | --- |
| Provider configuration | Official login, official login plus API, pure API, and aggregate providers; Responses / Chat Completions; model tests, model discovery, Provider Doctor, cc-switch and deep-link imports |
| Models and context | Per-model context windows, auto-compact limits, `model_catalog_json`, shared config, and per-provider MCP, Skill, and Plugin selection |
| Session management | Local session scanning, bulk deletion, Markdown export, token usage history, provider metadata sync and backups, session import and share links |
| Skills and scripts | Skills management and script marketplace installation/toggles |
| Codex enhancements | Plugin marketplace and model whitelist handling, paste fix, forced Chinese locale, fast startup, native menu localization, conversation width/scroll restore/thread IDs, service-tier controls, Goals, and image overlay |
| WeChat connection | Connect local Codex sessions through personal WeChat |
| Maintenance | App detection, shortcuts, Watcher, environment cleanup, logs, diagnostics, health checks, and Release update notifications |

Every UI enhancement is independently configurable. Disabling the global enhancement switch still leaves Codex++ available as a provider and launch manager.

## Provider Modes

Official login, mixed API, and pure API are stored and switched separately:

| Mode | Purpose | Authentication boundary |
| --- | --- | --- |
| Official login | Use only the official ChatGPT / Codex account | Removes custom providers and API keys while preserving official login state |
| Official login + API | Keep official account features and plugins while routing model requests to a compatible API | Stores the key as a provider bearer token, not in pure API `auth.json` |
| Pure API | Use a custom Base URL and key without an official account | Maintains independent `config.toml` and API-key auth without mixing official credentials |
| Aggregate provider | Route across multiple ordinary API providers | Supports failover, conversation round-robin, request round-robin, and weighted round-robin |

Each provider can configure Responses or Chat Completions, model lists, a test model, User-Agent, context windows, auto-compact limits, and enabled MCP servers, Skills, and Plugins. Chat Completions can be converted locally into the Responses protocol used by Codex.

Per-model windows accept values such as `1M`, `200K`, or plain integers. Codex++ generates a dedicated `model_catalog_json` for Codex.

Provider switching saves the current profile before applying the target profile. Real API keys remain local and should never be posted in logs, screenshots, or issues.

## Codex UI Enhancements

- Plugin marketplace unlock and model whitelist handling.
- Session delete, bulk delete, and Markdown export.
- Rich-text paste fix, forced Chinese locale, fast startup, and native menu localization.
- Conversation width, scroll restore, thread IDs, and service-tier controls.
- Per-provider Goals toggle written back to config.toml.
- Image overlay.

Most injected enhancements take effect after saving settings and restarting Codex++.

## Data Locations

- Codex config: `~/.codex/config.toml`
- Codex login state: `~/.codex/auth.json`
- Codex local database: `~/.codex/sqlite/*.db` first, falling back to legacy `~/.codex/state_5.sqlite`
- Codex++ state and logs: `~/.codex-session-delete/`
- Provider sync backups: `~/.codex/backups_state/provider-sync`

## Development

```bash
# Frontend checks
cd apps/codex-plus-manager
npm ci
npm run check
npm run vite:build

# Rust checks
cd ../..
cargo fmt --all -- --check
cargo test
cargo build --release
```

Main layout:

```text
apps/
  codex-plus-launcher/          Silent launch entry
  codex-plus-manager/           Tauri manager
assets/inject/
  renderer-inject.js            Enhancement script injected into Codex renderer
crates/
  codex-plus-core/              Launch, injection, config, update, install, and bridge logic
  codex-plus-data/              Session data, export, and provider sync
scripts/installer/
  windows/CodexPlusPlus.nsi     Windows NSIS installer
  macos/package-dmg.sh          macOS DMG packaging
```

## License

Upstream project copyright (C) 2026 BigPizzaV3. This fork continues under the [GNU Affero General Public License v3.0](LICENSE), SPDX identifier `AGPL-3.0-only`. Modified and distributed versions, including network-provided versions, must provide corresponding source under AGPLv3.

The license covers only CodexPlusPlus code and grants no rights to OpenAI, ChatGPT, or Codex trademarks, application assets, or other third-party content.

## Compatibility

Codex++ depends on the official desktop app's page structure, CDP, and local data formats. Some injected features may need adaptation after official app updates. Back up provider configuration or local session data before modifying it.
