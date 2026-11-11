# Release v0.4.0 — Tabs, Finished

Tabs arrived in 0.3.1, but only for the mouse, and a session came back as a single
document. This release finishes both: the strip behaves like a real tablist, and the
documents you had open are still there next time.

## 🚀 Key Improvements

### ⌨️ The Tab Strip, From the Keyboard

- **`Ctrl+Tab` / `Ctrl+Shift+Tab`** walk the strip and wrap around; **`Ctrl+W`** closes the document you are reading; a **middle-click** closes the tab under the pointer.
- **Left/right and Home/End** move within the strip and activate as they go, because the document is the point of the tab.
- **A real tablist, not a row of links**: each tab reports whether it is selected, and the strip is a single tab stop, so a screen reader announces a tab set and Tab does not walk through every close button before reaching the page.
- **Focus stays where you left it.** The strip is rebuilt whenever the tab set changes, and focus used to fall back to the page on that rebuild — which made the strip un-navigable after the first switch. Closing a tab now hands focus to its neighbour, and closing down to one document hands it back to the article.

### 🔁 The Documents You Had Open Come Back

- **Turn on *Reopen the documents I had open*** and a fresh launch rebuilds the strip, with the document you were reading in front. Twelve tabs at most.
- **Only the document on screen is read at start-up.** The other tabs are placeholders that load the first time you switch to them, so restoring five tabs costs one render rather than five.
- **Tabs whose file has moved or been deleted are pruned** before the strip is rebuilt, instead of failing one by one when you click them.
- **A file opened from Explorer still wins.** The stored session is only used when nothing was named on the command line, so double-clicking a file never gets overridden by what was open last time.

### 🧪 A Test Suite That Runs Itself

- **68 checks in a real browser**, driving the actual frontend inside the preview harness: the render path, the lightbox, the settings dialog, progressive rendering — and now the tab keyboard model, focus retention, middle-click, persistence and session restore.
- **CI runs it on every push**, in a headless browser, and fails the build when a check fails. No test framework, no bundler, nothing added to the size budget.
- **The formatter and the linter are gates now** (`cargo fmt --check`, `cargo clippy -D warnings`), after the one-time formatting pass the roadmap had been carrying since the first release.

### 🛠️ Under the Hood

- The session lives in `localStorage` beside the other preferences and is capped at twelve entries, so a stray list cannot turn start-up into a stampede of file reads.
- The strip is a `div` with `role="tablist"` rather than a `<nav>`: the role replaces the navigation landmark, and a tab is a wrapper around two sibling buttons because a button may not contain another button.
- The frontend stays three static files. The size budget moved from 108/62 KB to 112/66 KB to pay for the keyboard model and restore, and is enforced in CI.

---

## 📅 Recent Version History

- **v0.4.0**: Keyboard-driven tabs, session restore, front-end tests and a fmt/clippy gate.
- **v0.3.4**: Markdown files get a document icon of their own.
- **v0.3.3**: Settings panel, two-button toolbar, scroll and shift fixes.
- **v0.3.2**: Pinned section title, image lightbox, remembered window size and position.
- **v0.3.1**: Tabs — several documents at once in one window.
- **v0.3.0**: Syntax highlighting for fenced code, reopen-the-last-document, print styles removed.

---

**Full Changelog**: [CHANGELOG.md](https://github.com/n4od/.md-viewer/blob/main/CHANGELOG.md)