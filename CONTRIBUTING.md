# Contributing to Blend Stack

Thanks for helping out! Bug reports, ideas and pull requests are all welcome.

## Getting started

1. Install [Rust](https://rustup.rs), [Node.js](https://nodejs.org) 20+ and the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for your OS.
2. `npm install`
3. `npm run app:dev` — starts Vite and the Tauri app with hot reload.

## Before opening a pull request

```bash
npm run typecheck
cargo fmt --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml
```

## Guidelines

- **Official builds only.** Blend Stack downloads exclusively from Blender Foundation servers (`download.blender.org`, `builder.blender.org`). Please don't add third-party forks or mirrors.
- **Keep it fast.** Network and disk work belongs in Rust; the UI should never block on it. Cache what you fetch.
- **Keep it calm.** The design follows macOS conventions: system font sizes, soft materials, few colors, no clutter. New UI should feel like it was always there.
- **Translations.** Every user-facing string lives in `src/i18n/en.ts`. When you add one, add it to `src/i18n/ru.ts` too — TypeScript fails the build if a key is missing. New languages are welcome: copy `ru.ts`, translate, and register it in `src/i18n/core.ts`.
- One topic per pull request, with a short description of *why*.

## Reporting bugs

Please include your OS, Blend Stack version (Settings → About) and the log file:

| OS | Log folder |
| --- | --- |
| Windows | `%LOCALAPPDATA%\io.github.ilumetric.blendstack\logs` |
| macOS | `~/Library/Logs/io.github.ilumetric.blendstack` |
| Linux | `~/.local/share/io.github.ilumetric.blendstack/logs` |
