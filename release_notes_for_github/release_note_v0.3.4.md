# Release v0.3.4 — Markdown Files Get Their Own Icon

Markdown files no longer wear the app's tile in Explorer. They get a page of their own,
in the same palette and with the same mark.

## 🚀 Key Improvements

### 🎨 A File Icon, Not an App Icon

- **Documents look like documents**: a page with a folded corner, the app's serif "M" and its teal caret. A folder of notes now reads as notes, and sits clearly apart from the app tile instead of being a copy of it.
- **Drawn for the sizes Explorer actually uses**: 16px in list and detail views, 32px in tiles. The mark and the folded corner are chosen to survive that scale-down, where a detailed app tile turns to mush.
- **The "Open with" entry keeps the app icon**, deliberately: that row represents the app in the picker, not the document.

### 🛠️ Under the Hood

- **The artwork travels inside the binary** and is written out beside the theme, recent files and window geometry when the association is registered — so a *portable* copy gets the file icon too, with no second file to ship and nothing extra to clean up on uninstall.
- **A fresh install is corrected on first launch**, quietly, and the shell is told — so the new icon appears without restarting Explorer.
- **Still no new dependencies**: the icon comes from the project's own icon script, and a test pins the embedded bytes and the registry value's shape, so a silent fallback to the app tile cannot slip through unnoticed.

---

## 📅 Recent Version History

- **v0.3.4**: Markdown files get a document icon of their own.
- **v0.3.3**: Settings panel, two-button toolbar, scroll and shift fixes.
- **v0.3.2**: Pinned section title, image lightbox, remembered window size and position.
- **v0.3.1**: Tabs — several documents at once in one window.
- **v0.3.0**: Syntax highlighting for fenced code, reopen-the-last-document, print styles removed.

---

**Full Changelog**: [CHANGELOG.md](https://github.com/n4od/.md-viewer/blob/main/CHANGELOG.md)