# Roadmap

Where MD Viewer is going, and what it deliberately is not. The phases below are a
delivery order, not release numbers: what actually shipped, and in which release,
is recorded in [CHANGELOG.md](CHANGELOG.md).

## Guiding principles

These constrain every item below; an idea that breaks one needs an explicit
decision, not a pull request.

- **Read-only.** MD Viewer opens files and never writes them. Features that need
  an editor belong in another app.
- **No frontend framework, bundler, or web fonts.** Three static files stay
  three static files; Rust does the heavy lifting.
- **Optimise what we own.** Startup and memory are dominated by WebView2, so the
  budget is spent on the ~4.3 MB binary, the render path, and the first frame —
  not on chasing numbers the platform sets.
- **Windows first.** Other platforms are welcome once their association and
  packaging stories are actually exercised.

## Phase 1 — foundation (done)

Packaging, continuous integration, and the ground the reader stands on.

- [x] **Exercise the installer end to end.** Silent install, one
      `MDViewer.Markdown` ProgId pointing at the installed executable, a single
      named "Open with" entry, double-click launch, and an uninstall with no
      registry, install-directory or app-data residue. Re-run in Windows Sandbox
      (2026-11-11). The one branch a machine with WebView2 present cannot exercise
      is the download-bootstrapper, which only runs where the runtime is absent.
- [x] **Add CI** (GitHub Actions, `windows-latest`): `cargo check --all-targets`
      and `cargo test`, with a Cargo cache; green on `main`. `cargo fmt --check`
      and `cargo clippy -- -D warnings` still need a one-time formatting pass
      before they can join the workflow.
- [x] **Settle the CSP question for table alignment.** Alignment is emitted as
      `align-left|center|right` classes before sanitising and inline `style` is
      rejected outright, so the CSP can never block it. Covered by tests.
- [x] **Rename the bundle identifier** to `mdviewer` (was `dev.mdviewer.app`,
      which made Tauri warn about a macOS bundle-extension clash). A macOS target
      would need a reverse-DNS identifier of its own — see the macOS item below.
- [x] **CLI flags**: `--version`, `--help` and `--register-default` for scripted
      installs (`.wsb` logon commands, CI smoke tests), with exit codes and console
      output on demand; Windows still keeps the final default-app choice.
- [x] **Keep the last good render.** A failed reload keeps the article on screen
      and shows a notice naming the file, with a **Retry** action, instead of
      blanking the reading pane.
- [x] **Accessibility pass**: skip-to-content link; focus moves into the outline
      and find bar and returns to where it came from; the outline toggle reports
      `aria-expanded` and the current section `aria-current`; icon buttons carry
      labels; plain `j`/`k` no longer scroll while typing in the find field; muted
      text measures 4.65:1 (light) and 5.97:1 (dark), past WCAG AA.

## Phase 2 — reading and rendering (done)

The things you reach for while reading, and the render path underneath them.

- [x] **Find upgrades**: match-case and whole-word toggles (buttons, plus Alt+C /
      Alt+W), remembered across sessions, and a count that reports every occurrence
      even when only the first 1500 are highlighted.
- [x] **Heading anchors**: a "#" beside a heading, shown on hover or keyboard
      focus, copies a link to that section — the file path plus the heading's
      anchor (`C:\notes\a.md#setup`). The glyph is a CSS pseudo-element so it stays
      out of the heading's text, and headings keep a clean accessible name.
- [x] **Recent files housekeeping**: entries whose file no longer exists are
      pruned from the list and from storage on start-up, and there is a "Clear"
      action.
- [x] **Per-document reading position**, so reopening a long file returns to
      where you stopped. Written on the leading edge of a scroll burst (so closing
      the window right after scrolling cannot lose it) and refreshed when the burst
      settles; capped at 60 documents.
- [x] **Progressive rendering** for documents over ~1 MB. Measured first: opening
      is layout-bound, not parse-bound (a 1.2 MB document spent ~130 ms parsing and
      ~1.9 s in layout), so the lever is attaching the document in pieces rather
      than shortening `push_html`. The rendered HTML is split at top-level element
      boundaries and attached 128 KiB at a time, yielding to the browser between
      chunks: the first chunk paints in ~70 ms instead of after ~2.4 s, at the cost
      of ~25% more total time from the extra layout passes.
- [x] **Image placeholders**: local images gain their real `width`/`height` from
      the file header (PNG/JPEG/GIF/WebP, header only — never decoded; cached per
      render), so the browser reserves the correct box before the file loads and
      the text below no longer shifts. Author values win, remote/`data:` images are
      skipped, and a render with no document folder does no I/O. Rust-side, so it
      costs nothing in the frontend size budget.
- [x] **Session continuity.** With no file on the command line, the welcome screen
      offers the last document as its lead action ("Reopen sample.md"), so resuming
      is one click. Nothing is persisted and there is no auto-open setting: the last
      document is simply the top of the recent list, which avoids a switch that
      could leave the welcome screen — the only place to turn it off — unreachable.

## Phase 3 — exploring

Not a commitment: each open item here wants a decision before it starts.

- [ ] **macOS and Linux**: packaging plus associations (macOS `Opened` events,
      Linux `.desktop` `MimeType`). Different flows from Windows; needs its own
      verification matrix, not a port of the registry code. macOS also needs a
      reverse-DNS bundle identifier (`mdviewer` is Windows/Linux-shaped).
- [ ] **Auto-update** via `tauri-plugin-updater`, signed bundles, and a release
      host. Needs signing keys and a decision about where update metadata lives.
- [ ] **Math and diagrams** (KaTeX / Mermaid): high value for notes, high cost for
      the zero-dependency frontend. Only as opt-in, lazily loaded assets.
- [x] **Syntax highlighting** for fenced code, done in Rust on the render path: a
      small tokeniser for the languages a note is likely to use (Rust, JS/TS,
      Python, JSON, shell, SQL, YAML/TOML, CSS, HTML/XML, the C family), emitting
      class spans the stylesheet colours. No highlighter bundle and no client-side
      work; unknown languages are left untouched. Measured cost on a code-heavy
      document: ~26% more render time and ~15% more payload. Every token colour
      measures at least 4.5:1 against the code background in both themes.
- [ ] **UI localisation** for the chrome strings (the rendering path is already
      language-agnostic).

## Not planned

Listed so the answer is explicit, not to invite debate:

- **Editing, saving, formatting** — the product is a reader.
- **Printing and PDF export** — a reader with no print action has no route to it.
  Print styles were written and then removed again rather than left in the sheet
  unreachable.
- **Tabs or multiple windows** — one window, one document, enforced by the
  single-instance plugin.
- **Telemetry, accounts, or network calls** at startup. Remote images in a
  document are the user's content, not our traffic.
- **Bundling a browser engine** (Electron-style) — WebView2 is the platform.
- **A "total memory < 30 MB" target** — that requires dropping the webview
  entirely; the measured tradeoff is documented in [PERFORMANCE.md](PERFORMANCE.md).
- **A plugin system** — the security surface is not worth it for a viewer.

## Contributing to the roadmap

Open an issue on GitHub ([@n4od](https://github.com/n4od)) before starting
anything in *Phase 3*. User-visible changes get an entry in
[CHANGELOG.md](CHANGELOG.md), and the verification habits in
[AGENTS.md](AGENTS.md) apply to roadmap work too.
