# Release v0.5.0 — A Quieter Top of the Window

The reader got quieter. The window used to stack three bands of chrome above the text and
say the same things in each of them; now it is one row, and the second band only appears
when it has something to say.

## 🚀 Key Improvements

### 🧹 One Band Instead of Three

- **The tab strip moved into the toolbar.** A window with tabs open used to reserve 108px above the text — a toolbar, a tab strip and a pinned section bar, stacked. It now reserves **48px**: the tabs sit in the toolbar row next to the buttons, and nothing docks below it.
- **The toolbar stopped repeating itself.** The document name was in the title bar, again in the toolbar and again in the active tab; the folder path was in the toolbar and again in the footer; the word count was in both places too. What is left is the tab strip and two buttons — the name lives in the title bar and the tab, the path and the word count in the footer.
- **The app mark is gone from the toolbar.** It duplicated the window icon two inches to its left.
- **The pinned section bar now earns its place.** It used to sit under the toolbar naming the very heading you were already reading; it appears only once that heading has scrolled out of view, and disappears again at the top of a document.
- The footer carries the reading stats together: words, reading time, path and timestamp.

### ⌨️ Ctrl + Wheel Zooms

- **The gesture every reader has.** `Ctrl` (or `Cmd`) plus the wheel scales the article on the same 80–160% scale as `Ctrl` `+` / `-` / `0`, with smaller steps because a wheel repeats faster than a key. The event is intercepted, so the webview does not page-zoom instead.

### 🖱️ The Heading `#` Is Gone

- The small `#` that appeared beside a heading on hover, and copied a link to that section, has been **removed** — from the markup, the styles and the click handler.
- Heading ids stay, so the outline and in-document links (`[text](#heading)`) behave exactly as before.

### 🛠️ Under the Hood

- **One layout variable fewer.** `--tabs-h` existed to keep everything docked below the tab strip in step; with the strip inside the toolbar, headings' scroll margin, the outline drawer and the pinned bar all key off `--topbar-h` alone.
- **The front-end suite grew from 68 checks to 76**: the sticky bar's show/hide rule, the toolbar carrying no duplicates, and four checks for the wheel zoom (a plain wheel must *not* zoom, Ctrl+wheel in, out, and settling at the clamp).
- **Still three static files**, and the size budget did not have to move: `src/` is 106.5 KB of its 112 KB, `app.js` 63.9 KB of 66 KB — the removals paid for the addition.

### 📦 Beyond the App

- The repository is now **n4od/md-viewer** (from a name that began with a dot).
- **Contributing is easier from any machine**: the UI runs with nothing but Node (`node scripts/dev-server.js`), the Rust side is verified on Ubuntu as well as Windows in CI, and the docs no longer assume this maintainer's PC.
- `CONTRIBUTING.md`, `SECURITY.md`, `CODE_OF_CONDUCT.md` and issue/PR templates landed since 0.4.0.

---

## 📅 Recent Version History

- **v0.5.0**: One band of chrome, Ctrl+wheel zoom, and the heading `#` removed.
- **v0.4.0**: Keyboard-driven tabs, session restore, front-end tests and a fmt/clippy gate.
- **v0.3.4**: Markdown files get a document icon of their own.
- **v0.3.3**: Settings panel, two-button toolbar, scroll and shift fixes.
- **v0.3.2**: Pinned section title, image lightbox, remembered window size and position.
- **v0.3.1**: Tabs — several documents at once in one window.

---

**Full Changelog**: [CHANGELOG.md](https://github.com/n4od/md-viewer/blob/main/CHANGELOG.md)
