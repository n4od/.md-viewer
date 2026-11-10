# AGENTS.md

Tauri v2 + Rust Markdown viewer. Frontend is three static files; the whole
backend is one Rust file. Windows is the only exercised target.

## Owner rules (non-negotiable)

- **Never run git write commands without explicit approval.** No `add`, `commit`,
  `tag`, `push`, `init`, `checkout`, `reset`. Read-only git (`status`, `diff`,
  `log`) is fine. Commits are the owner's job.
- **Do not run release or full builds unless asked.** `cargo build --release`,
  `tauri build`, `npm run bundle` and installer runs take 3–7 minutes. Use
  `cargo check` (seconds, warm cache) and focused `cargo test` instead. If a
  release build is genuinely needed, ask first.
- **No new frontend dependencies, bundlers, or frameworks.** The no-bundler
  design is deliberate; `package.json` exists only for the Tauri CLI.

## Commands (from repo root unless noted)

| Purpose | Command |
| :-- | :-- |
| Fast compile gate | `cargo check --manifest-path src-tauri/Cargo.toml` |
| All tests | `cargo test --manifest-path src-tauri/Cargo.toml` (17 pass, 1 ignored) |
| One test | `cargo test --manifest-path src-tauri/Cargo.toml open_command -- --nocapture` |
| UI iteration | `node scripts/dev-server.js` → <http://localhost:8123/preview/harness.html> |
| Desktop layout | <http://localhost:8123/preview/wide.html> (1280×900 iframe) |
| Manual doc | `src-tauri\target\release\mdviewer.exe sample.md` (existing binary only) |

- Always invoke scripts as `powershell -ExecutionPolicy Bypass -File scripts\...`.
  `npm.ps1`/`npx.ps1` are blocked by execution policy on this machine — use
  `npm.cmd` / `npx.cmd`, and note `node_modules` is absent, so `npm run ...`
  needs `npm install` first.
- There is no CI, no lint/format config, and no typechecker. Don't invent
  `npm run lint`. `cargo check` + tests are the real gate.
- HTML/CSS/JS are embedded at compile time (`frontendDist: ../src`), so plain
  `cargo run` shows stale assets after frontend edits. Use the preview harness,
  or `npx tauri dev` (serves `src/` from disk) when the CLI is installed.

## Architecture (non-obvious parts)

- `src-tauri/src/main.rs` is the entire backend (~1.2k lines, sections in order:
  paths → reading → Markdown → loading → default handler → watcher → commands →
  setup). Unit tests live in the same file at the bottom.
- Render path: `pulldown-cmark` (GFM) → HTML string → `ammonia` allow-list →
  `Document` payload to the webview. **Heading ids are added in JS**
  (`enhance()`), because the renderer emits plain `<h1>`; the outline and
  in-document anchors depend on that.
- Sanitiser details that break styling if changed: inline `style` survives only
  for `text-align` (table alignment), footnote markup is
  `div.footnote-definition` + `sup.footnote-reference`, alerts are
  `markdown-alert-<kind>` (not `markdown-alert`).
- The window starts `visible: false`; Rust shows it when the frontend calls the
  `ready` command (3 s watchdog). Don't set `visible: true` — that reintroduces
  the white flash. The startup file is read and rendered in `setup()` before the
  webview paints and handed over via `initial_document`.
- Drag & drop arrives as `tauri::WebviewEvent::DragDrop`, **not** `WindowEvent`.
- Asset protocol scope in `tauri.conf.json` is intentionally empty; Rust grants
  the open document's folder at runtime. Don't widen it in config.
- CSP is `script-src 'self'` / `style-src 'self'`: no inline `<script>` (and
  `app.js` must stay in `<head>` so the theme is applied before first paint).
  The preview harness has **no CSP**, so verify CSP-sensitive changes in the real
  app, not just the harness.
- `windows = "0.62.2"` is pinned to the version `tauri-runtime-wry` uses so
  `WebviewWindow::hwnd()` (an `HWND`) can be passed to shell APIs. Bumping it
  alone will break the interop.
- `tauri-plugin-single-instance` must stay the first registered plugin; its
  callback is what makes a second double-click switch documents in the running
  window.

## File association (easy to get wrong)

- The "Open with" list is built from
  `HKCU\Software\Classes\Applications\<exe>`. Without `FriendlyAppName` the entry
  renders blank (or is dropped), which looks like the registration failed.
- `Explorer\FileExts\<ext>\OpenWithList` is a letter-keyed MRU; `MRUList` holds
  the order. **Merge, never overwrite** — both the Rust and PowerShell paths do.
- `assoc`/`ftype` only read the machine hive, so they show nothing for this
  per-user registration. Use `scripts/show-open-with.ps1` (SHAssocEnumHandlers).
- Windows reserves the default handler (`UserChoice`, hash-checked). The app
  registers + calls `SHOpenWithDialog`; the user's confirmation is required. Do
  not "fix" this by writing `UserChoice`.
- The registered path must come from the running executable
  (`std::env::current_exe()`), so portable/moved/Sandbox copies register
  themselves.
- Both the registration script and the ignored test write **real HKCU keys**:
  `cargo test ... -- --ignored association_registration_round_trip`, then
  `scripts\unregister-file-association.ps1 -Force`. The test registers the test
  binary, so re-register the real exe afterwards if the machine should keep the
  association (`scripts\register-file-association.ps1 -SetDefault`).

## Verification habits

- `sample.md` + `sample-notes.md` exercise every renderer path (tables, tasks,
  alerts, footnotes, definition lists, relative links, local images).
- Preview harness specifics: it fakes `window.__TAURI__`, duplicates
  `src/index.html` markup (keep in sync), and exposes `window.__previewEmit` /
  `window.__previewPayload` for driving events.
- `scripts/measure.ps1` **kills any running `mdviewer` process** at the start and
  end, then measures timing/memory. Only pass `-WriteReport` when you intend to
  rewrite `PERFORMANCE.md`.
- Measured facts live in `PERFORMANCE.md`; don't restate performance claims that
  aren't there. WebView2 dominates (~0.9–1.0 s to visible window, ~132 MB private
  for its process tree); the Rust process is ~22 MB.

## Docs and versions

- `README.md` is the user-facing source of truth for build/package/association
  steps; `CHANGELOG.md` records history; `ROADMAP.md` is forward-looking. Update
  them for user-visible changes.
- Licence is MIT (`LICENSE`, `Cargo.toml`, `package.json`). Don't relicense
  without an explicit instruction.
- Version `0.1.0` lives in three places: `src-tauri/Cargo.toml`,
  `package.json`, `src-tauri/tauri.conf.json`. Keep them in sync.
- Icons are generated (`scripts/make-icons.ps1`) — don't hand-edit PNG/ICO.
  `src-tauri/gen/schemas/` is generated; leave it alone. `Cargo.lock` is present
  and not ignored: commit it (binary crate).
