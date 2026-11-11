# Release v0.3.0 — Syntax Highlighting & Session Continuity

Fenced code is now coloured — in ten-plus languages, with no highlighter bundle and
no client-side JavaScript — and the welcome screen leads with the document you were
last reading, so picking up where you left off is one click.

## 🚀 Key Improvements

### 🚀 Syntax Highlighting

- **Ten-plus languages out of the box**: Rust, JavaScript and TypeScript, Python, JSON, shell, PowerShell, SQL, YAML and TOML, CSS, HTML/XML and the C family (which covers C, C++, Java, C#, Go, Swift and Kotlin fences).
- **Nothing added to the frontend**: tokenising happens in Rust on the render path, so `app.js` is unchanged — no highlighter bundle, no client-side work, and the "three static files" promise is intact.
- **Sanitiser still has the last word**: spans are produced before the allow-list runs, so a highlighted block cannot introduce anything the sanitised renderer would have rejected. Markup inside a string stays text.
- **Colours that pass AA**: every token colour measures at least 4.5:1 against the code background in both themes (worst case 4.90 light, 4.66 dark).
- **Tolerant by design**: a language the highlighter does not know, or a fence with no language tag, renders exactly as it did before. Only the interesting tokens get a span, so plain identifiers and punctuation keep the normal code colour and the payload stays lean.

### ✨ Resuming Work

- **Reopen the last document**: starting with no file on the command line, the welcome screen now leads with "Reopen `<name>`" for the document that was last on screen. It is one click instead of finding it in the recent list.
- **Nothing to switch off**: the last document is simply the top of the recent list, so there is no autostart setting and no way to get stuck with one you cannot turn off.

### 🧹 Removed

- **Print styles**: MD Viewer is a reader and exposes no print action, so the `@media print` rules shipped in 0.2.0 were removed rather than left in the stylesheet unreachable. The stylesheet is now screen-only and about 2.3 KB lighter.

### 🛠️ Developer / Core

- **The cost is measured, not guessed**: on a deliberately code-heavy document (600 Rust blocks), 124 KiB of Markdown now renders to 357 KiB of HTML in ~94 ms — about +26% render time and +15% payload against the same document before highlighting.
- **No new dependencies**: the highlighter is a small tokeniser in `main.rs` with a table per language, and it is covered by tests alongside the rest of the render path.
- **CI budget raised deliberately**: the frontend ceiling moves from 80/46 KB to 88/52 KB after the 0.2.0 batch reached the old one. The guard still rules out a bundler or a framework.

---

## 📅 Recent Version History

- **v0.3.0**: Syntax highlighting for fenced code, reopen-the-last-document, print styles removed.
- **v0.2.0**: Progressive rendering for large files, section links, image sizing, find toggles, reading position, print fixes.
- **v0.1.1**: Retry on a failed reload, recent-list housekeeping, `--version` / `--help` / `--register-default` flags.
- **v0.1.0**: First release — a read-only, single-window Markdown viewer with Windows file association.

---

**Full Changelog**: [CHANGELOG.md](https://github.com/n4od/.md-viewer/blob/main/CHANGELOG.md)