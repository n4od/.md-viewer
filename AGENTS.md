# AGENTS.md

Engineering notes for MD Viewer: a Tauri v2 + Rust Markdown viewer whose frontend is
three static files and whose backend is one Rust file. Written for coding agents, and
useful to anyone changing the internals.

> Machine-specific rules for a particular checkout live in `AGENTS.local.md`, which is
> gitignored. If that file exists, it takes precedence over anything here. Nothing below
> depends on one machine: commands are given in their portable form.

## Project rules (non-negotiable)

- **No new frontend dependencies, bundlers, or frameworks.** The no-bundler design is
  deliberate; `package.json` exists only for the Tauri CLI. The size budget in CI is what
  keeps that honest.
- **Read-only.** The app renders Markdown and never writes the document. Anything that
  needs an editor belongs in another application.
- **Windows is the packaged target.** The Rust code is `cfg`-gated for other platforms
  and CI proves it compiles on Linux, but the installer, the registry association and
  WebView2 are Windows-only concerns.

## Commands (from repo root unless noted)

| Purpose | Command |
| :-- | :-- |
| Fast compile gate | `cargo check --manifest-path src-tauri/Cargo.toml` |
| All tests | `cargo test --manifest-path src-tauri/Cargo.toml` (33 pass, 1 ignored) |
| Front-end tests | `node scripts/dev-server.js` → <http://localhost:8123/preview/tests.html> (91 checks, `RESULT: PASS`) |
| One test | `cargo test --manifest-path src-tauri/Cargo.toml open_command -- --nocapture` |
| UI iteration | `node scripts/dev-server.js` → <http://localhost:8123/preview/harness.html> |
| Desktop layout | <http://localhost:8123/preview/wide.html> (1280×900 iframe) |
| Manual doc | `src-tauri/target/release/mdviewer.exe samples/sample.md` (existing Windows binary only) |

- The `.ps1` scripts (icons, association, measurement) are Windows-only maintenance
  tools; contributing does not require them. Run them as
  `powershell -ExecutionPolicy Bypass -File scripts/<name>.ps1` on a machine whose
  policy blocks local scripts.
- `npm install` is needed once for the Tauri CLI (`tauri dev`, `tauri build`). The
  frontend does not need it: `node scripts/dev-server.js` has no dependencies.
- CI lives in `.github/workflows/ci.yml` and has two jobs. **windows-latest** is the full
  gate: a **frontend size budget** — `src/` ≤ 118 KB, `app.js` ≤ 70 KB, raised from
  80/46 KB when the 0.2.0 batch reached that ceiling, to 96/56 for tabs and the Phase 4
  items, to 108/62 for the settings panel, to 112/66 for the tab keyboard model and
  session restore, and to 118/70 for the frameless title bar — then **the front-end
  tests** in a headless browser, then
  `cargo fmt --check`, `cargo check --all-targets`, `cargo clippy -- -D warnings`, then
  `cargo test`. **ubuntu-latest** installs Tauri's Linux prerequisites, then runs the
  **front-end suite** in headless Chromium, `cargo check --all-targets` and `cargo test`,
  which is what keeps the "the Rust side is portable and so is the suite" claim honest;
  it does not build a package.
  The test step starts `scripts/dev-server.js` itself, tries Edge then Chrome, dumps
  the settled DOM of `/preview/tests.html`, and fails unless the page reports
  `data-result="pass"`;
  browsers are GUI-subsystem binaries, so `--dump-dom` output has to be redirected
  to a file rather than captured from the pipe. There is still no
  lint/format/typecheck config — don't invent `npm run lint`; `cargo check` + both
  test suites are the gate. The size budget is the guard behind "three static files,
  no framework": if a change needs more room, raise it in the same commit and say why.
- HTML/CSS/JS are embedded at compile time (`frontendDist: ../src`), so plain
  `cargo run` shows stale assets after frontend edits. Use the preview harness,
  or `npx tauri dev` (serves `src/` from disk) when the CLI is installed.

## Architecture (non-obvious parts)

- `src-tauri/src/main.rs` is the entire backend (~1.2k lines, sections in order:
  paths → reading → Markdown → loading → default handler → watcher → commands →
  setup). Unit tests live in the same file at the bottom.
- Render path: `pulldown-cmark` (GFM) → HTML string → `ammonia` allow-list →
  `Document` payload to the webview. **Heading ids are added in JS** (`enhance()`),
  because the renderer emits plain `<h1>`; the outline and in-document anchors depend
  on that, so the ids stay even though the old hover-"#" copy-link affordance was
  removed: a reader does not need to copy section links.
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
- Tabs are frontend state. `tabs`/`activePath` in `app.js` own the strip, and
  `rememberTab()` — called from `render()` — upserts by path, so every route to a
  document (dialog, recent list, link, drop, a second double-click, a reload) lands
  in it. Rust still tracks exactly one active document: `prepare_document` is the
  only writer of `Viewer.current`, and switching a tab is just `load_path`, which
  re-points the watcher, the asset scope and the window title for free. The strip lives
  **inside the toolbar row** (`#topbar`), hidden unless two documents are open: one band
  of chrome, and `--tabs-h` no longer exists, so nothing docks below the toolbar. The
  offsets that used to add it key off the scroll container instead: headings'
  `scroll-margin-top` is `calc(var(--sticky-h) + 18px)`, because `#main` is the scroller
  and starts below the toolbar. Because the window is frameless that row is also the title
  bar, so the name the strip carries at two documents is carried by `.win-title` at
  one — `renderTabs()` sets both, and `title.hidden = multiple` is the switch. A tab is
  a wrapper holding two sibling buttons, because a
  button may not contain another button. The strip is a
  `role="tablist"` (a `div`, not a `<nav>`: the role replaces the landmark), each tab
  button is a `role="tab"` with `aria-selected` and a roving `tabindex`, and the
  wrapper is `role="presentation"`. **`renderTabs()` rebuilds every node**, so it
  captures which control had focus and restores it afterwards, preferring
  `focusAfterRender` (set by an activation or a close); without that the strip lost
  focus on the first switch and could not be navigated again. Closing down to one tab
  clears and hides the strip, so tab-count assertions must read `strip.hidden` rather
  than `querySelectorAll(".tab").length`. `saveTabs()` mirrors the set into
  `mdviewer.tabs` (`{ paths, active }`, only when there are two or more), and
  `restoreTabs()` rebuilds it as placeholders — only the stored active document is
  read, the rest load on first activation — after `filter_existing` prunes paths whose
  file is gone. It runs only when nothing was named on the command line.
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
- **The window is frameless** (`decorations: false`, `shadow: true`), so `#topbar` is
  the title bar: `data-tauri-drag-region="deep"` makes the band draggable and
  double-clicking it maximizes, both handled by Tauri's own script — which already
  refuses to drag from anything clickable (`button`, `a`, `[tabindex]`, `role="tab"`),
  which is why the tabs and the buttons need no opt-out. Tauri does not strip the frame
  styles to make the window frameless — the live window still reports both `WS_CAPTION`
  and `WS_THICKFRAME` (covered, not removed) — and that is what keeps edge resizing, the
  DWM shadow and snapping working; `tauri-runtime-wry`'s `undecorated_resizing` subclass
  hit-tests the edges itself. `drag_window()` sends `WM_NCLBUTTONDOWN(HTCAPTION)`, which
  is the native drag, so drag-to-edge snapping still works; the Windows 11 snap-layouts
  *flyout* is the one thing lost, as it is drawn by the OS around the caption button we no
  longer have.
  `src-tauri/capabilities/default.json` grants exactly four extra window permissions
  (`start-dragging`, `minimize`, `toggle-maximize`, `close`) — `is-maximized` and
  `internal-toggle-maximize` are already in `core:window:default`. The webview can still
  do nothing else; keep it that way rather than adding `core:window:default` wholesale.
- **The page does not scroll; `#main` does.** `html`/`body` are a fixed-height flex column
  and `#main` is the only scroll container. That is what puts the scrollbar below the
  toolbar instead of beside it, and what lets the toolbar span the full window width — so
  the window buttons sit flush against the window edge. Three consequences: the scroll
  listener is on `#main` (scroll events do not bubble), `headingTops` reads `offsetTop`
  relative to `#main` (which is positioned, so it *is* the `offsetParent`), and every read
  or move of the reading position goes through `readingTop()`/`readingHeight()`/`readingTo()`/
  `readingOffset()` in `app.js` — never `window.scrollY`, which no longer means anything.
  The pinned bar stays a `position: fixed` overlay below the toolbar (out of flow, so it
  never shifts the text), and the settings dialog's scroll lock is `html.modal-open #main`.
- Drag & drop arrives as `tauri::WebviewEvent::DragDrop`, **not** `WindowEvent`.
- Asset protocol scope in `tauri.conf.json` is intentionally empty; Rust grants
  the open document's folder at runtime. Don't widen it in config.
- CSP is `script-src 'self'` / `style-src 'self'`: no inline `<script>` (and
  `app.js` must stay in `<head>` so the theme is applied before first paint).
  The preview harness has **no CSP**, so verify CSP-sensitive changes in the real
  app, not just the harness — `preview/tests.html` inherits the same blind spot, so a
  green suite does not clear an inline-style or inline-script question.
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
- **The file-type icon is not the app icon.** Windows draws a document's icon from
  its ProgId, so `MDViewer.Markdown\DefaultIcon` points at
  `%LOCALAPPDATA%\mdviewer\markdown.ico` — artwork carried in the binary
  (`FILE_ICON`, from `scripts/make-icons.ps1`) and written out at registration, which
  is what lets a portable copy have it too. `Applications\mdviewer.exe\DefaultIcon`
  deliberately keeps the *app* icon, because that row represents the app in the
  picker. Tauri's installer writes the executable's icon on the ProgId, so
  `ensure_file_icon()` quietly corrects it once on the first launch; the ignored
  registry test and `register-file-association.ps1` both do the same, and the
  `.txt` ProgId (installer-only) keeps the app icon for the same picker reason.
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
  `scripts/unregister-file-association.ps1 -Force`. The test registers the test
  binary, so re-register the real exe afterwards if the machine should keep the
  association (`scripts/register-file-association.ps1 -SetDefault`).

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
- Build with `npm install` then `npx tauri build` (or `npm run bundle`);
  output is `src-tauri/target/release/bundle/nsis/*-setup.exe`. Silent flags:
  `/S`. `currentUser` install dir: `%LOCALAPPDATA%\MD Viewer`.
- For scripted or headless registration call `mdviewer.exe --register-default`
  (writes the per-user keys for the running executable, exit 0/1, no UI). It is
  what a `.wsb` logon command or a CI smoke test should use instead of the
  PowerShell script.
- **Testing the installer mutates shared state.** It overwrites the
  `MDViewer.Markdown` keys the portable build depends on, and the uninstall hook
  deletes `%LOCALAPPDATA%\mdviewer` (theme, recent files and the remembered window
  geometry — `window.json`, written next to the WebView2 profile). Export the
  HKCU keys first if they matter, and re-run
  `scripts/register-file-association.ps1 -SetDefault` afterwards.
- The bundle identifier is `mdviewer` (was `dev.mdviewer.app`, which made the CLI
  warn about a macOS bundle-extension clash). A bare identifier is fine on Windows;
  macOS wants reverse-DNS, so a macOS target would need one of its own.
- `package-lock.json` comes from `npm install` and should be committed;
  `node_modules/` and `src-tauri/target/` are ignored.

## Verification habits

- `samples/sample.md` + `samples/sample-notes.md` exercise every renderer path (tables,
  tasks, alerts, footnotes, definition lists, relative links, local images);
  `samples/highlight-demo.md` does the same for fenced code — one block per language the
  highlighter knows, plus the traps (markup inside a string, an unknown language, no
  language tag at all). The images in `samples/sample.md` live in `samples/images/` and
  are referenced relatively, because the asset protocol grants the document's own folder
  and nothing above it: an image outside that folder is refused, and the document shows a
  broken image rather than an error. Keep sample images inside the samples folder.
- Preferences live in `localStorage` under `mdviewer.*`: `theme`, `scale`, `width`,
  `resume`, `reopen`, `geometry`, `find`, `recent`, `positions` and `tabs` (the stored
  session). The settings panel
  is the only UI over them — a native `<dialog>`, so Esc, focus trapping and focus
  return are the platform's job rather than hand-rolled — and it re-reads its state on
  every open through `syncSettings()`. The two actions it shares with the welcome
  screen click those buttons instead of duplicating the flow. The one preference Rust
  needs is "remember window geometry": the frontend sends it with
  `set_remember_geometry` and Rust keeps the flag in `window-remember.txt` beside the
  geometry, so `setup()` can honour it before the webview exists.
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
  window, ~138 MB private for its process tree); the Rust process is ~23 MB.

## Docs and versions

- `README.md` is the front door: what the app is, a screenshot, install and the
  keyboard. The depth lives in `docs/` — `architecture.md` (layout, Rust render path,
  IPC, frontend), `file-association.md`, `build-and-package.md` (builds, installer,
  registration, the verification checklist) and `performance.md`. Screenshots are
  committed under `docs/images/`. `CHANGELOG.md` records history; `ROADMAP.md` is
  forward-looking. Update the relevant one for user-visible changes — a change to how
  the app is built or registered belongs in `docs/build-and-package.md`, not the
  README.
- Licence is MIT (`LICENSE`, `Cargo.toml`, `package.json`). Don't relicense
  without an explicit instruction.
- Version `0.5.0` lives in five places, four of them mechanical:
  `src-tauri/Cargo.toml`, `package.json`, `src-tauri/tauri.conf.json`,
  `package-lock.json` (`npm install` rewrites it) and the `app_version` stub in
  `preview/harness.html`. The README's `--version` example quotes it too. Grep for
  the previous version rather than trusting memory.
- Icons are generated (`scripts/make-icons.ps1`) — don't hand-edit PNG/ICO.
  `src-tauri/gen/schemas/` is generated; leave it alone. `Cargo.lock` is present
  and not ignored: commit it (binary crate).
