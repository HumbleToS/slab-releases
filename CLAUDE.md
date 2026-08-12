# CLAUDE.md — Slab

Single source of truth for this repo. Read fully before any work. If reality and this file disagree, update this file in the same PR.

## Project

Slab: native Windows dashboard app for the HYTE Y70 Touch case screen (1100x3840 portrait touch panel). Rust + Tauri v2. Replaces a prototype that used HTML + a PowerShell GSMTC sidecar; see `design-plan.md` for full architecture, product pillars, and milestones. The design plan is authoritative for scope.

Product pillars, in priority order: touch-first, fully customizable, zero-friction install. Every PR should be able to answer which pillar it serves.

## Stack (non-negotiable)

- Rust stable, Tauri v2. Cargo package name is `slab-app` (`slab` is taken on crates.io by the allocator crate); product/binary name is Slab.
- `windows` crate for GSMTC media integration — no polling sidecars, no synthetic keypresses
- Frontend: vanilla HTML/CSS/JS in the Tauri webview. No frontend framework.
- pnpm for the frontend toolchain and Tauri CLI
- Config: `config.toml` in the platform config dir, serde + toml, hot-reloaded via `notify`
- Weather: Open-Meteo (keyless). No API keys anywhere in this project.

## Commands

- `pnpm tauri dev` — run with hot reload
- `pnpm tauri build` — release build + bundle
- `pnpm check` — fmt + clippy gate (`cargo fmt --check` + `cargo clippy --all-targets -- -D warnings`); must pass before handing work back. Windows-native.
- `./scripts/check-wsl.sh` — the same gate runnable from WSL/Linux, cross-checking the `x86_64-pc-windows-msvc` target (zig rc stands in for the resource compiler)
- `./scripts/test-wsl.sh` — cross-links the unit tests via cargo-xwin and executes them on the Windows host through WSL interop
- `./scripts/build-win.sh` — cross-builds the release Windows exe from WSL (clang-cl + lld-link + xwin SDK + zig rc); `pnpm tauri build` on a real Windows machine is the equivalent
- `./scripts/release-win.sh` — builds the signed NSIS installer and publishes it plus the `latest.json` update feed to github.com/HumbleToS/slab-releases; installed apps self-update within six hours. Bump `version` in tauri.conf.json first.

## Conventions

- VCS (maintainer decision 2026-08-11, superseding the 2026-08-10 local-only rule): the source lives at github.com/HumbleToS/slab-releases — the same repo that hosts the installers and `latest.json` update feed, so release-download URLs stay unchanged. Solo-maintainer workflow (2026-08-11): committing and pushing straight to `main` is fine — no PRs or branches required. Conventional commit messages (`feat:`, `fix:`, `chore:`, `docs:`), gated by `pnpm check` before every push.
- Backend modules: `media.rs` (GSMTC), `weather.rs`, `display.rs` (monitor detection), `config.rs` (load/watch/defaults), `theme.rs` (token resolution), `commands.rs` (Tauri IPC surface). Keep IPC thin — logic lives in the modules, not in command handlers.
- State flows backend → frontend via Tauri events; intents flow frontend → backend via commands. The webview makes no network calls — weather goes through the backend too.

## Theming rule (the customization contract)

- The frontend never hardcodes a themable value. Colors, opacity, blur, radius, font, text scale, background, and shade all route through CSS custom properties populated from `[theme]` in config.toml.
- Adding any new visual element means adding its values to the theme pipeline, with the v2 default documented in design-plan.md. A hardcoded hex color in CSS is a PR rejection.
- Config contract: unknown keys ignored, unknown widget kinds skipped with a log line, missing/corrupt config regenerates defaults, saves hot-reload without restart. Never crash on user config.

## Touch rule (the interaction contract)

- Every interaction must work by touch alone: no hover-dependent UI, no right-click-only actions, no keyboard-only paths.
- Touch targets ≥ 90px, pressed states on everything interactive, native inertia on scrollable regions.
- Suppress OS text-selection and long-press callout artifacts on the dashboard surface.
- The window must not steal focus from the user's foreground app when touched. Verify on hardware — this goes on the HUMAN-TODO checklist every milestone. (2026-08-11: the original always-on-bottom requirement is disabled — HWND_BOTTOM sinks the dashboard beneath Wallpaper Engine's wallpaper layer, making it invisible on machines running WE. The panel is a dedicated surface; normal z-order + WS_EX_NOACTIVATE is correct. Revisit only with a WorkerW-aware z-order dance.)

## Guardrails

- Never commit secrets. There should be no secrets in this project at all — if a feature seems to need one, stop and raise it in HUMAN-TODO.md.
- `{{CONFIRM}}` tokens mark decisions requiring the maintainer's explicit sign-off (code signing, anything that costs money). CI treats their presence in shipped artifacts as a launch blocker. Never resolve one yourself.
- Anything requiring human action (cert purchase, manual test on the reference machine, SAC verification, touch-on-hardware checks) goes in `HUMAN-TODO.md`, not in code comments. `HUMAN-TODO.md` is local-only and gitignored — it names people and machines and must never be pushed.
- Privacy: nothing personally identifying goes into tracked files or commit metadata — no real names, personal emails, or machine paths. Commits use the GitHub noreply identity (repo-local git config).
- Exit test for M1 is sacred: fresh Win11 + Smart App Control ON + installer = working dashboard, zero script warnings, zero manual config, and a live theme change via config.toml save. Any change that would add a setup step for the end user needs a design-plan amendment first.
- Do not add telemetry, auto-update phone-home, or network calls beyond Open-Meteo without a `{{CONFIRM}}`. Maintainer-approved 2026-08-10: the updater's check against github.com/HumbleToS/slab-releases is the one approved additional endpoint.
- The updater signing key lives OUTSIDE the repo at `~/.tauri/slab-updater.key` (public key in tauri.conf.json). Never move it into the tree; losing it permanently breaks updates (see HUMAN-TODO).

## Testing

- Unit-test config parsing (unknown kinds/keys, corrupt file → defaults), theme token resolution, and weather-code mapping
- GSMTC, display enumeration, and touch behavior can't run in CI — cover them with a manual test checklist in `HUMAN-TODO.md` per milestone
- Frontend: no test framework; keep logic in the backend where it's testable

## Current milestone

M1 — parity, themable. Scope is exactly the M1 list in `DESIGN-PLAN.md` (including the 2026-08-10 amendment — minimal on-dashboard customization panel writing through config.toml — the 2026-08-11 amendment — `stats` and `text` widget kinds pulled forward from M2 — and the 2026-08-12 amendments — widget add/remove/reorder plus full param editing in the customization window; text_scale scoped to the panel surface). Do not pull other M2/M3 features forward.