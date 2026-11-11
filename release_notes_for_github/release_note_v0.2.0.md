# Release v0.2.0 — Large Files, Section Links & Print Fixes

This release makes MD Viewer comfortable with documents it used to choke on —
multi-megabyte notes now paint in a fraction of a second — and adds the small
things you reach for while reading: links to a section, find toggles that stick,
a reading position that survives closing the window, and a print stylesheet that
actually works.

## 🚀 Key Improvements

### 🚀 New Features

- **Progressive rendering for large files**: Documents above 1 MiB of rendered HTML are now attached 128 KiB at a time instead of in one go, so the top of the file appears while the rest is still being built. A document that took ~2.4 s to show anything now paints its first chunk in ~70 ms.
- **Same document, same DOM**: Chunking is provably lossless — the chunks rejoin byte-for-byte and produce an identical DOM to a single insert, so nothing about the rendered result changes.
- **Safe to interrupt**: Opening a new file mid-render cancels the old one cleanly; a superseded chunk pump stops on its next step rather than appending pieces of the previous document.
- **Section links**: Hover a heading (or Tab to it) and a `#` appears; clicking it copies a link to that section as the file path plus the anchor, e.g. `C:\notes\a.md#setup`. It copies rather than navigates, so it never scrolls you away from where you are.
- **Image placeholders**: Local images now carry their real pixel dimensions, read from the file header, so the page reserves the correct box before the image loads and the text below no longer jumps.
- **Find toggles**: Match-case and whole-word switches in the find bar (also `Alt+C` / `Alt+W`), remembered across sessions.
- **Find counting that stays honest**: The counter reports every occurrence; only the first 1500 become highlight ranges, so a document with thousands of hits stays responsive while the number stays truthful.
- **Reading position**: Reopening a long document returns to where you stopped. The position is written on the leading edge of a scroll burst — so closing the window right after scrolling cannot lose it — with a trailing debounce for the resting position, capped at 60 documents.

### ⚡ Performance

- **Outline scroll-spy**: Heading offsets were re-measured for every heading on every scroll frame, so the cost grew with document size. They are now measured once per layout and binary-searched: on 500 headings the per-tick cost fell from 0.75 ms to about 0.0004 ms.
- **Cheaper document open**: An ASCII fast path in `slugify` (the Unicode property escapes were re-run for every heading) and one delegated outline listener instead of one per link cut a 500-section document's open from 67 ms to 42 ms plain, and 131 ms to 75 ms with tables and code.
- **Instant first paint on large files**: The first chunk of a big document is on screen in ~70 ms, where previously the whole document had to be parsed, enhanced and laid out before anything appeared.
- **Profile first, then optimise**: Laying out a 1.2 MB document turned out to be ~1.9 s of the ~2 s open, against ~130 ms of parsing — which is why the fix attaches the document in pieces rather than shortening the Markdown render.

### ♿ Accessibility

- **AA contrast in both themes**: The faintest text token now measures 4.65:1 on the light theme and 5.97:1 on dark, past WCAG AA.
- **Skip to content**: A skip link lets keyboard users jump past the toolbar straight into the article.
- **Focus that goes somewhere and comes back**: The outline and the find bar move focus in when opened and hand it back to where it came from when closed.
- **States that are announced**: The outline toggle reports `aria-expanded`, the current section `aria-current`, the find match count is a live region, and the theme button names the active mode.
- **Keyboard guards**: Plain `j`/`k` no longer scroll the page while you are typing in the find field.
- **Clean heading names**: Section anchors are built so a screen reader announces the heading text, not the copy-link control attached to it.

### 🐛 Bug Fixes

- **Printing in dark mode**: The print styles now force the light palette, so a reader in dark mode no longer prints near-white text on white paper.
- **No clipped content on paper**: Wide tables and long code lines scroll sideways on screen, which meant they were simply cut off when printed. They now wrap, and tables drop to 9pt with cell wrapping.
- **Page breaks that respect content**: Table header rows repeat on every page a table spans, and breaks are avoided inside code blocks, tables, rows and images.
- **Predictable page margins**: A `@page` rule sets a 16 mm × 18 mm margin instead of relying on the dialog default.

### 🛠️ Developer / Core

- **Frontend size budget in CI**: The build fails if `src/` exceeds 80 KB or `app.js` exceeds 46 KB, so "three static files, no framework" is enforced by the pipeline rather than remembered.
- **Dead ends recorded in the code**: Staging the document off-DOM before attaching it, and delegating the code-copy buttons, were both measured and rejected — the numbers are kept in the source so they are not retried.
- **Image dimensions without a dependency**: PNG, JPEG, GIF and WebP sizes are read from the file header only (never decoded), from the first 64 KiB, and cached per render.
- **Still zero dependencies**: Every feature above ships in the same three static frontend files and the same single Rust binary — no new runtime, no web fonts, no network calls.

---

## 📅 Recent Version History

- **v0.2.0**: Progressive rendering for large files, section links, image sizing, find toggles, reading position, and print fixes.
- **v0.1.1**: Retry on a failed reload, recent-list housekeeping, and the `--version` / `--help` / `--register-default` command-line flags.
- **v0.1.0**: First release — a read-only, single-window Markdown viewer with first-class Windows file association.

---

**Full Changelog**: [CHANGELOG.md](https://github.com/n4od/.md-viewer/blob/main/CHANGELOG.md)
