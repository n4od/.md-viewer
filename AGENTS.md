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
| All tests | `cargo test --manifest-path src-tauri/Cargo.toml` (23 pass, 1 ignored) |
| One test | `cargo test --manifest-path src-tauri/Cargo.toml open_command -- --nocapture` |
| UI iteration | `node scripts/dev-server.js` → <http://localhost:8123/preview/harness.html> |
| Desktop layout | <http://localhost:8123/preview/wide.html> (1280×900 iframe) |
| Manual doc | `src-tauri\target\release\mdviewer.exe sample.md` (existing binary only) |

- Always invoke scripts as `powershell -ExecutionPolicy Bypass -File scripts\...`.
  `npm.ps1`/`npx.ps1` are blocked by execution policy on this machine — use
  `npm.cmd` / `npx.cmd`, and note `node_modules` is absent, so `npm run ...`
  needs `npm install` first.
- CI lives in `.github/workflows/ci.yml` (windows-latest: a **frontend size budget**
  — `src/` ≤ 88 KB, `app.js` ≤ 52 KB, raised from 80/46 KB when the 0.2.0 batch
  reached the old ceiling — then `cargo check --all-targets`, then
  `cargo test`). There is still no lint/format/typecheck config — don't invent
  `npm run lint`; `cargo check` + tests are the gate. The size budget is the guard
  behind "three static files, no framework": if a change needs more room, raise it
  in the same commit and say why.
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
  in-document anchors depend on that. `enhance()` also adds the hover "#" anchor:
  its glyph is a CSS pseudo-element so it never enters the heading's
  `textContent` (ids, outline labels and find read that text), and the heading's
  `aria-label` is pinned to its visible text so the link is not appended to every
  heading a screen reader announces. The anchor's offset is `rem`, not `em` — an
  `em` offset scales with the heading and pushes an `h1`'s glyph past the article's
  padding.
- Local images get `width`/`height` from their file header (`image_dimensions`:
  PNG/JPEG/GIF/WebP, no decoding, first 64 KiB only, cached per render) so the
  browser reserves the box before the file loads. Author-supplied dimensions win
  and non-local `src`s are skipped. `.markdown img` pairs `max-width: 100%` with
  `height: auto`, which is what lets the attributes reserve space without
  distorting. Only `render_markdown_in` with a document folder does this; passing
  `None` (most unit tests) does no filesystem I/O.
- Fenced code is highlighted in Rust, not in the browser: `highlight_code_blocks`
  finds each `<pre><code class="language-x">`, unescapes the text, tokenises it with
  a per-language table (`language_for`), and writes class-carrying spans back before
  `sanitize` runs. Only the interesting tokens get a span (comments, strings,
  numbers, keywords, and markup tags/attributes), which keeps the payload down, and
  the class names are deliberately short (`tok-c`, `tok-s`, `tok-n`, `tok-k`,
  `tok-t`, `tok-a`) because they repeat per token. A language it does not know is
  left exactly as the renderer emitted it. Colours are `--tok-*` in both themes and
  are held to WCAG AA against `--code-bg`. The fixture in `preview/harness.html`
  mirrors this output by hand so the CSS can be iterated without a rebuild.
- Documents above 1 MiB of HTML are attached in 128 KiB chunks by
  `renderProgressive()`; `render()` (the small path) and it share `finishRender()`
  for everything that needs the whole document in the DOM. `enhance(root, state)`
  takes an optional shared `{ used, found }` so heading ids stay unique across
  chunks, and `splitTopLevelHtml()` only cuts where element nesting is back at the
  top level (a split mid-tag would corrupt the document). Each render bumps
  `renderToken`; a pump that a newer render superseded stops on its next step.
  Chunks yield with `requestAnimationFrame` — a visible frame is what makes the
  first chunk paint — but the seam is `window.__TAURI__.scheduler.next`, which the
  harness replaces with a synchronous runner because rAF is frozen in an occluded
  tab. Don't switch the yield to `MessageChannel`: it fires, but a fast task chain
  does not guarantee a paint.
- Sanitiser details that break styling if changed: no inline `style` survives —
  table alignment is converted to `align-left|center|right` classes *before*
  sanitising, because the CSP blocks inline styles; footnote markup is
  `div.footnote-definition` + `sup.footnote-reference`; alerts are
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

## Windows installer (NSIS)

- `bundle.fileAssociations[].name` **is** the ProgId on Windows: Tauri's template
  passes it into `Software\Classes\<name>`. It must equal `PROG_ID` in `main.rs`
  (a test enforces this). Putting a display string there silently creates a second
  ProgId, and "Open with" then lists the app twice.
- The template does **not** write `Applications\<exe>` (which supplies
  `FriendlyAppName` — the picker's label) or `OpenWithProgids`, and its uninstall
  leaves `<ProgId>_backup` values behind and can restore an extension default that
  points at the ProgId it deleted. `src-tauri/windows/hooks.nsh` (wired through
  `nsis.installerHooks`) handles all of it. Defines available in hooks:
  `${PRODUCTNAME}`, `${MANUFACTURER}`, `${MAINBINARYNAME}`, `${BUNDLEID}`, `SHCTX`.
- Build with `npm install` then `npx.cmd tauri build` (or `npm run bundle`);
  output is `src-tauri/target/release/bundle/nsis/*-setup.exe`. Silent flags:
  `/S`. `currentUser` install dir: `%LOCALAPPDATA%\MD Viewer`.
- For scripted or headless registration call `mdviewer.exe --register-default`
  (writes the per-user keys for the running executable, exit 0/1, no UI). It is
  what a `.wsb` logon command or a CI smoke test should use instead of the
  PowerShell script.
- **Testing the installer mutates shared state.** It overwrites the
  `MDViewer.Markdown` keys the portable build depends on, and the uninstall hook
  deletes `%LOCALAPPDATA%\mdviewer` (theme + recent files). Export the
  HKCU keys first if they matter, and re-run
  `scripts\register-file-association.ps1 -SetDefault` afterwards.
- The bundle identifier is `mdviewer` (was `dev.mdviewer.app`, which made the CLI
  warn about a macOS bundle-extension clash). A bare identifier is fine on Windows;
  macOS wants reverse-DNS, so a macOS target would need one of its own.
- `package-lock.json` comes from `npm install` and should be committed;
  `node_modules/` and `src-tauri/target/` are ignored.

## Verification habits

- `sample.md` + `sample-notes.md` exercise every renderer path (tables, tasks,
  alerts, footnotes, definition lists, relative links, local images);
  `highlight-demo.md` does the same for fenced code — one block per language the
  highlighter knows, plus the traps (markup inside a string, an unknown language, no
  language tag at all).
- Preview harness specifics: it fakes `window.__TAURI__`, duplicates
  `src/index.html` markup (keep in sync), and exposes `window.__previewEmit` /
  `window.__previewPayload` for driving events.
- `scripts/measure.ps1` **kills any running `mdviewer` process** at the start and
  end, then measures timing/memory. Only pass `-WriteReport` when you intend to
  rewrite `PERFORMANCE.md`.
- Measured facts live in `PERFORMANCE.md`; don't restate performance claims that
  aren't there. (That file is regenerated wholesale by `measure.ps1 -WriteReport`
  and covers startup and memory, so render-path numbers belong with the change —
  `CHANGELOG.md` — not in that file.) WebView2 dominates (~1.1 s to visible
  window, ~136 MB private for its process tree); the Rust process is ~22 MB.

## Docs and versions

- `README.md` is the user-facing source of truth for build/package/association
  steps; `CHANGELOG.md` records history; `ROADMAP.md` is forward-looking. Update
  them for user-visible changes.
- Licence is MIT (`LICENSE`, `Cargo.toml`, `package.json`). Don't relicense
  without an explicit instruction.
- Version `0.3.0` lives in three places: `src-tauri/Cargo.toml`,
  `package.json`, `src-tauri/tauri.conf.json`. Keep them in sync.
- Icons are generated (`scripts/make-icons.ps1`) — don't hand-edit PNG/ICO.
  `src-tauri/gen/schemas/` is generated; leave it alone. `Cargo.lock` is present
  and not ignored: commit it (binary crate).
