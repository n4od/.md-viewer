# Roadmap

Where MD Viewer is going, and what it deliberately is not. Status as of
**0.1.0** (2026-11-10): read-only viewer, Windows-verified, associations
installable and settable from inside the app.

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

## Near term — 0.2

Small, verifiable, and mostly finishing what 0.1.0 started.

- [ ] **Exercise the installer end to end.** NSIS is configured but has not been
      run in a clean VM or Windows Sandbox: associations, WebView2 bootstrap on an
      image that lacks it, and uninstall leaving no registry or file residue.
- [ ] **Add CI** (GitHub Actions, `windows-latest`): `cargo check`, `cargo test`,
      then `cargo fmt --check` and `cargo clippy -- -D warnings` once the tree is
      clean. Cache `~/.cargo` and `target/` between runs.
- [ ] **Settle the CSP question for table alignment.** `style-src 'self'` likely
      blocks the inline `style="text-align:…"` that table cells carry; verify in
      the real app (the preview harness has no CSP) and either allow
      `style-src-attr` narrowly or emit a class instead. Add a regression check.
- [ ] **Session continuity.** With no file argument, offer to reopen the last
      document (opt-in, stored alongside the recent files).
- [ ] **CLI flags**: `--version` and `--register-default` for scripted installs
      and Sandbox `.wsb` logon commands, with meaningful exit codes (the release
      binary has no console, so don't print to stdout).
- [ ] **Keep the last good render.** If a watched file is deleted or renamed
      mid-edit, show a non-destructive notice instead of replacing the article.
- [ ] **Accessibility pass**: focus order through the top bar, outline and find
      bar; screen-reader labels for the icon buttons; contrast check for muted
      text in both themes; keyboard-only walkthrough of every action.

## Next — 0.3

- [ ] **Find upgrades**: whole-word and case-sensitivity toggles, and a match
      count that stays accurate for very long documents.
- [ ] **Heading anchors**: hover link plus "copy link to section", next to the
      outline.
- [ ] **Recent files housekeeping**: drop entries whose file no longer exists,
      and a "clear list" action.
- [ ] **Print/PDF polish**: verify page breaks for code blocks and tables, and
      hide the outline/find chrome reliably (print styles exist; untested on paper).
- [ ] **Per-document reading position**, so reopening a long file returns to
      where you stopped.
- [ ] **Progressive rendering** for documents over ~1 MB: paint the first chunk
      and continue, instead of one long `push_html` call.
- [ ] **Image placeholders**: reserve space from image dimensions where available
      to avoid layout shift while local images load.

## Later — exploring

- [ ] **macOS and Linux**: packaging plus associations (macOS `Opened` events,
      Linux `.desktop` `MimeType`). Different flows from Windows; needs its own
      verification matrix, not a port of the registry code.
- [ ] **Auto-update** via `tauri-plugin-updater`, signed bundles, and a release
      host. Needs signing keys and a decision about where update metadata lives.
- [ ] **Math and diagrams** (KaTeX / Mermaid): high value for notes, high cost for
      the zero-dependency frontend. Only as opt-in, lazily loaded assets.
- [ ] **Syntax highlighting** for fenced code, if it can be done without shipping
      a highlighter bundle — a small tokenizer or build-time classes.
- [ ] **UI localisation** for the chrome strings (the rendering path is already
      language-agnostic).

## Not planned

Listed so the answer is explicit, not to invite debate:

- **Editing, saving, formatting** — the product is a reader.
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
anything in *Later*. User-visible changes get an entry in
[CHANGELOG.md](CHANGELOG.md), and the verification habits in
[AGENTS.md](AGENTS.md) apply to roadmap work too.
