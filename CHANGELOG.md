# Changelog

All notable changes to MD Viewer are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and
the project uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.3.1] - 2026-11-11

### Added

- **Tabs.** More than one document can be open at once: open, close and switch
  between them in the one window. The strip only appears once a second document is
  open, so reading a single document looks exactly as it did before, and each tab
  keeps its own scroll position, section marker and find query. This reverses the
  earlier "tabs or multiple windows" decision — multiple windows stay declined,
  because a window is a whole WebView2 process tree (~320 MB working set measured)
  while extra tabs cost nothing beyond the documents themselves. Switching a tab is
  `load_path`, so the file watcher, the asset scope and the window title follow the
  active document without any new Rust state.

## [0.3.0] - 2026-11-11

### Added

- **Syntax highlighting for fenced code.** The renderer tokenises a fenced block
  when it knows the language (Rust, JS/TS, Python, JSON, shell, PowerShell, SQL,
  YAML/TOML, CSS, HTML/XML, and the C family) and emits class-carrying spans; the
  stylesheet colours them, and every token colour measures at least 4.5:1 against
  the code background in both themes. There is no highlighter bundle and no
  client-side tokenising — the work happens in Rust on the render path, and a
  language it does not know is left exactly as it was. Measured cost on a
  deliberately code-heavy document: 124 KiB of Markdown now renders to 357 KiB of
  HTML in ~94 ms, against ~69 ms and 310 KiB before.
- **Reopen the last document.** Starting with no file on the command line, the
  welcome screen offers the document that was last on screen as its lead action
  ("Reopen sample.md"), so resuming is one click. The last document is simply the
  top of the recent list — nothing extra is stored, and there is no auto-open
  setting that could leave you with no way to turn it off.

### Changed

- The CI frontend size budget moves from 80/46 KB to 88/52 KB (`src/` total and
  `app.js`). The 0.2.0 batch of user-facing features took the frontend from 60.7 KB
  at v0.1.1 to 82.2 KB of dependency-free code, reaching the old ceiling; the guard
  still rules out a bundler or a framework.

### Removed

- **Print styles.** MD Viewer is a reader and exposes no print action, so the
  `@media print` rules and the `@page` margin that shipped in 0.2.0 are gone; the
  stylesheet is now screen-only and about 2.3 KB lighter. Printing was never
  reachable from the app, so this changes nothing for anyone using it.

## [0.2.0] - 2026-11-11

### Added

- **Accessibility pass**: muted text now meets WCAG AA in both themes (the faintest
  token measures 4.65:1 on light and 5.97:1 on dark); a skip-to-content link; the
  outline and find bar move focus in on open and hand it back on close; the outline
  toggle reports `aria-expanded`, the current section `aria-current`, the find
  match count is a live region, and the theme button names the active mode.
- **Per-document reading position**: reopening a long document resumes where it was
  left. Written on the leading edge of a scroll burst (so closing the window right
  after scrolling cannot lose it) with a trailing debounce for the resting
  position, keyed by path and capped at 60 documents.
- **Find upgrades**: match-case and whole-word toggles in the find bar (also
  Alt+C / Alt+W), persisted across sessions. The counter now reports every
  occurrence; only the first 1500 are turned into highlight ranges, so a document
  with thousands of hits stays responsive while the count remains truthful.
- **Progressive rendering for large documents.** Above 1 MiB of rendered HTML the
  document is split at top-level element boundaries and attached 128 KiB at a
  time, yielding to the browser between chunks, so the top of the file paints in
  tens of milliseconds instead of after the whole thing has been laid out. A
  document that took ~2.4 s to appear now shows its first chunk in ~70 ms (and
  finishes a few hundred milliseconds later — the cost of the extra layout
  passes). Appending the chunks produces the same DOM as inserting the HTML in one
  go, and a render that is superseded (a newer file opened mid-pump) stops
  cleanly.
- **Heading anchors.** A "#" appears beside a heading on hover (or keyboard focus)
  and copies a link to that section — the file path with the heading's anchor, e.g.
  `C:\notes\a.md#setup`. The glyph is a CSS pseudo-element, so it never leaks into
  the heading's text (which the ids, the outline labels and find all read), and each
  heading keeps a clean accessible name rather than announcing the copy link.
- **Image placeholders.** A local image now carries its real pixel dimensions,
  read from the file header (PNG, JPEG, GIF, WebP — no decoding, no new
  dependency), so the page reserves the correct box before the file loads and the
  text no longer jumps when it appears. Author-supplied `width`/`height` still win,
  and remote or `data:` images are left alone.

### Changed

- The outline's scroll-spy measures heading offsets once per layout and
  binary-searches them, instead of reading `offsetTop` for every heading on every
  frame. On a 500-heading document that took the per-tick cost from 0.75 ms (and
  scaling with document size) to about 0.0004 ms, at the price of one 0.7 ms
  measurement pass whenever the layout changes (open, resize, an image loading).
- CI enforces a frontend size budget (`src/` ≤ 80 KB, `app.js` ≤ 46 KB), so the
  "three static files, no framework" promise is checked rather than remembered.
- Opening a document got a little cheaper. `slugify` now takes an ASCII fast path
  (the unicode property escapes were re-run for every heading), and the outline
  uses one delegated click listener instead of one per link. On a 500-section
  document the whole open measured about 42 ms plain / 75 ms mixed (best of six),
  down from 67 ms / 131 ms; a profiled breakdown puts parsing at ~4 ms and each
  enhancement pass at 1–3 ms, so the remainder is the layout pass the browser owes
  the visible article.
- Two ideas were measured and rejected, and the numbers are recorded in the code
  so they are not retried: building the article in a detached container and moving
  it in was *slower* (211 ms vs 131 ms on an 80 KiB document — the extra tree move
  buys no layout savings, because the browser already batches layout within a
  task), and delegating the code-copy buttons or building the toolbar with
  `innerHTML` saved nothing and cost more respectively.

### Fixed

- **Printing in dark mode.** The print styles now force the light palette, so a
  reader in dark mode no longer prints near-white text on white paper (the palette
  is resolved to a concrete `data-theme` before first paint, so the print block has
  to override `:root[data-theme]`, not just `body`).
- **Print clipping.** Wide tables and long code lines scrolled sideways on screen
  and were simply cut off on paper; they now wrap (tables also drop to 9pt, and
  their header rows repeat across pages). Breaks are avoided inside code blocks,
  tables, rows and images, and a page margin is set.

## [0.1.1] - 2026-11-11

### Added

- **Retry** on the document error card: a reload that fails (file deleted or
  renamed by an editor mid-save) keeps the last good render on screen and names
  the file instead of blanking the reading pane. Clicking Retry re-reads it.
- **Recent-list housekeeping**: entries whose file no longer exists are dropped on
  start-up, and the list has a **Clear** action. Backed by a new `filter_existing`
  command, so the check happens in Rust rather than trusting the webview.
- **Command line flags**: `--version` (`-V`), `--help` (`-h`) and
  `--register-default`. The last writes the per-user association keys for the
  running executable and refreshes the shell without opening a window, which is
  what scripted installs (a Windows Sandbox `.wsb` logon command, a CI smoke test)
  need. The binary re-attaches to the parent console, so the output appears when it
  is run from a terminal even though it ships as a windowed app.

### Changed

- The bundle identifier is `mdviewer` instead of `dev.mdviewer.app`, which made the
  bundle step warn that the `.app` suffix collides with macOS bundles. Note the
  side effect: WebView2's data directory moves to `%LOCALAPPDATA%\mdviewer`, so the
  stored theme and recent-files list reset once on the next run.
- Recorded that the NSIS installer was exercised end to end and re-run inside
  Windows Sandbox (2026-11-11), which supersedes the 0.1.0 note that the installer
  path had not been exercised.
- Publishing guidance lives in the `github-publish-release` skill; the repository
  deliberately keeps no second copy of it.

## [0.1.0] - 2026-11-10

First release: a read-only, single-window Markdown viewer built with Tauri v2 and
Rust, with first-class Windows file association. The app is a single ~4.3 MB
executable (WebView2 runtime aside).

### Added

**Viewer**

- Markdown rendered in Rust with `pulldown-cmark` (CommonMark plus GFM tables,
  task lists, strikethrough, footnotes, alerts, definition lists, superscript and
  subscript); YAML/TOML-style front matter is parsed and hidden.
- Startup path capture: `std::env::args()` is scanned for a file argument, so
  double-clicking a `.md` in Explorer opens it directly. Handles quoted paths,
  `file://` URLs, switches and `--`.
- The startup document is read and rendered during `setup()`, before the webview
  exists; the window is created hidden and shown after the first paint (with a
  3-second watchdog so it can never stay invisible).
- Single-instance behaviour via `tauri-plugin-single-instance`: opening another
  file focuses the running window and switches documents instead of spawning a
  second one.
- Opening paths dynamically through a native file dialog (`Ctrl+O`) and through
  drag & drop, with a full-window drop overlay.
- Relative links between Markdown files open in place; external `http(s)`/`mailto`
  links go to the OS handler; `javascript:` and `data:` links are ignored.
- Live reload with `notify`: the document's **folder** is watched (atomic saves by
  editors replace the file), events are debounced by 180 ms, and the re-render
  preserves scroll position. A toast confirms the reload.
- Local images referenced relatively are served through Tauri's asset protocol;
  the document's folder is granted recursively at open time and nothing else is.
- Recently opened files (last 6, per user) on the empty state, stored in
  `localStorage`.

**File association**

- `bundle.fileAssociations` in `tauri.conf.json` for `md`, `markdown`, `mdown`,
  `mkd`, `mkdn`, `mdwn`, `mdtxt`, `mdtext` (plus `txt`), with a per-user NSIS
  installer.
- `scripts/register-file-association.ps1` / `...unregister-file-association.ps1`
  for unpackaged builds. Writes, under `HKCU\Software\Classes`: the ProgId and its
  icon/command, `Applications\<exe>` with `FriendlyAppName`, `ApplicationIcon`,
  `SupportedTypes`, per-extension `OpenWithProgids`, Explorer's `OpenWithList` MRU
  and `ApplicationAssociationToasts`. No administrator rights required.
- In-app **Set as default for .md files** action (`make_default`), with a
  `default_status` command driving a live status line on the empty state. It
  registers the *currently running* executable (so portable/moved/Sandbox copies
  register themselves), calls `SHChangeNotify`, then opens the Windows
  "How do you want to open this file?" picker via `SHOpenWithDialog` with
  `OAIF_ALLOW_REGISTRATION | OAIF_REGISTER_EXT`.
- `scripts/show-open-with.ps1`, which lists what the shell actually offers for an
  extension by calling `SHAssocEnumHandlers` — the API the picker itself uses.

**Interface**

- Clean reading surface: system font stack, serif display headings, 46 rem
  measure, automatic light/dark following the OS, plus a manual
  system → light → dark cycle (`Ctrl+Shift+L`) resolved before first paint.
- Outline drawer built from the rendered headings (ids are slugified in the
  webview), with scroll-spy highlighting (`Ctrl+B`).
- Find in document using the CSS Custom Highlight API, so matches never mutate the
  DOM (`Ctrl+F`, Enter / Shift+Enter, `Esc`).
- Reading progress bar, word count and reading-time estimate, path/timestamp
  footer, code blocks with language label and copy button, task-list checkboxes,
  styled alerts, tables with preserved column alignment, print stylesheet.
- Reading-size scaling (`Ctrl` + `+` / `-` / `0`), `J`/`K` page scrolling, smooth
  scrolling with `scroll-margin` for the sticky bar, and `prefers-reduced-motion`
  support.
- Empty, error and loading states, including an inline error when a reloaded file
  disappears.

**Developer tooling**

- `scripts/make-icons.ps1` generates the PNG/ICO icon set with GDI+ (no external
  tools); 7 sizes in `icon.ico`.
- `scripts/measure.ps1` reports time-to-visible-window and memory for the app
  process and its WebView2 children, optionally writing `PERFORMANCE.md`.
- `preview/harness.html` + `scripts/dev-server.js`: the real `index.html` /
  `styles.css` / `app.js` run in any Chromium browser against a fake Tauri bridge,
  so UI work does not require a Rust rebuild. `preview/wide.html` forces a
  1280×900 viewport for desktop-layout checks.
- 19 unit tests covering argument parsing, `file://` handling, BOM/UTF-16
  decoding, truncation, GFM rendering, sanitising, alignment, open-command
  quoting, ProgId matching, picker arguments, installer/config consistency and
  render speed. One opt-in integration test (`--ignored`) writes and verifies the
  real registry keys.
- Repository metadata: MIT `LICENSE`, `ROADMAP.md`, author credit, and
  `AGENTS.md` notes for future contributors.

**Packaging**

- NSIS installer hooks (`src-tauri/windows/hooks.nsh`, wired through
  `nsis.installerHooks`) that write the `Applications\<exe>` entries
  (`FriendlyAppName`, icon, command, `SupportedTypes`) and the per-extension
  `OpenWithProgids` candidates Tauri's template omits. They also make uninstall
  remove those keys, the `*_backup` values, a stale extension default and
  WebView2's data folder.
- CI (`.github/workflows/ci.yml`): `cargo check --all-targets` and `cargo test`
  on `windows-latest`, with a Cargo cache.

### Changed

- `fileAssociations[].name` is the runtime ProgId (`MDViewer.Markdown` /
  `MDViewer.Text`) rather than a display string, so the installer, the in-app
  action and the scripts register one handler instead of two.
- The installer path is verified end to end: silent install, a single ProgId,
  one named "Open with" entry, double-click launch, and an uninstall with no
  registry, install-directory or app-data residue. The WebView2
  download-bootstrapper path stays untested (no machine without the runtime).
- Performance figures in the README point at `PERFORMANCE.md` instead of copying
  one run's numbers, which drift between runs.

### Security

- Raw HTML in documents is allow-list sanitised with `ammonia`: no `<script>`,
  `<iframe>`, `<svg>`, `<form>`, no `on*` handlers, no `javascript:`/`data:`
  URLs; inline styles are dropped entirely (table alignment is emitted as a
  class, because the CSP blocks inline styles).
- CSP locks the webview down (`default-src 'self'`, `script-src 'self'`,
  `object-src 'none'`, `base-uri 'none'`, `form-action 'none'`, IPC-only
  `connect-src`).
- The frontend has no filesystem plugin and no shell access; all file reads happen
  in Rust. The capability file grants only `core:default`.
- The asset protocol scope starts empty and is extended per document folder, so an
  opened file cannot expose unrelated directories.
- External navigation is filtered by scheme in Rust before reaching the OS opener.

### Fixed

Issues found and fixed during verification, before the first release:

- The "Open with" list did not show MD Viewer: the `Applications\<exe>` entry
  lacked `FriendlyAppName`, so it was rendered blank or dropped. Fixed, together
  with the icon, `SupportedTypes`, MRU and toast entries.
- Registration overwrote Explorer's per-extension `OpenWithList` order, displacing
  other apps. It now merges: reuses an existing slot, takes the first free letter,
  and repairs a truncated `MRUList`.
- `scripts/register-file-association.ps1` aborted at the MRU step because
  `'a'..'z'` is not valid PowerShell (the range operator is integer-only).
- Uninstall left `Applications\mdviewer*.exe` keys, MRU letters and the toast
  values Windows writes (`Applications\mdviewer.exe_.md`,
  `MDViewer.Markdown_.md`) behind; teardown now removes all of them while leaving
  other applications untouched.
- GFM alert styling never applied: `pulldown-cmark` emits
  `markdown-alert-<kind>`, not `markdown-alert`.
- Footnote and table-alignment styling was written against markup the renderer
  does not produce; both were corrected and covered by tests.
- Heading ids are generated in the webview (slugified, de-duplicated), so the
  outline and in-document anchors work even though `pulldown-cmark` emits plain
  `<h1>`–`<h6>` without `id` attributes.
- `winreg` 0.56 stores value bytes as `Cow<[u8]>`; the `OpenWithProgids` value is
  written as `REG_NONE` accordingly.
- Removed a hanging COM `IAssocHandler::Invoke` probe (no message pump) from the
  tooling rather than shipping a foot-gun.

- Table column alignment relied on an inline `style` attribute, which the app's
  own CSP (`style-src 'self'`) blocks, so it never rendered in the app. Alignment
  is emitted as `align-left|center|right` classes before sanitising, and inline
  styles are now rejected outright.
- The installer derived its ProgId from the association display name
  (`"Markdown Document"`), so a machine that also had the per-user registration
  ended up with two MD Viewer entries in "Open with" and two competing ProgIds.
- Uninstall left the `*_backup` registry values behind, could leave an extension
  default pointing at the ProgId it had just deleted, and never removed WebView2's
  data folder. The uninstall hook now finishes all three.
- Removed a dead variable in the webview's link click handler.

### Measured

Recorded on the development machine with the release build (see `PERFORMANCE.md`,
reproducible with `scripts/measure.ps1`):

| Metric | Value |
| :-- | :-- |
| Portable executable | 4.35 MB |
| Time to visible window | 891–1118 ms across runs |
| `mdviewer.exe` working set / private | 22–23 MB / ~5 MB |
| WebView2 processes (working set sum / private) | 291–298 MB / ~132 MB |
| Render 124 KiB Markdown (release) | 56.7 ms |
| Render `sample.md` (8 KiB) | ~4 ms |

### Known limitations

- Read-only by design: no editing, no saving, no multiple documents per window.
- Files larger than 16 MB are truncated with a visible notice.
- Remote images (badges) load over the network; tighten `img-src` for a fully
  offline viewer.
- Windows is the verified target. macOS/Linux code paths exist but are untested
  here, and in-app association registration is Windows-only.
- Windows reserves the choice of default application for the user: the in-app
  action registers the app and opens the system picker, but the final
  confirmation is always a user action.
- The NSIS installer path is configured and documented; it had not been exercised
  at the time of this entry (verified afterwards — see `[Unreleased]`).
