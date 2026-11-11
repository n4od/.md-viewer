# Release v0.3.3 — Settings, and a Smaller Toolbar

Every preference the app had been keeping to itself now has a home, the toolbar is down
to two buttons, and the reading pane holds still.

## 🚀 Key Improvements

### ⚙️ Settings

- **One panel for every preference**: theme, text size (until now keyboard-only), reading width, whether the reading position is remembered, whether the last document reopens at start, the default find options, the recent list, the file types the app claims along with the default-app action, remembered window size and position, and an About line with the running version.
- **"Reopen the last document at start" is now a real setting.** It had been left as a one-shot offer on the welcome screen, because a persisted switch with no way to turn it off would have been a trap — the panel is a permanent home for it.
- **It behaves the way a dialog should**: Esc closes it, focus returns to the gear that opened it, and a click outside dismisses it.

### 🎨 A Smaller Toolbar

- **Two buttons instead of four**: contents and settings. The theme button went with the setting it duplicated (`Ctrl+Shift+L` still cycles the theme), and the open-file button went because `Ctrl+O`, drag and drop, the welcome screen and Explorer already cover opening a file.

### 🐛 Fixes

- **The reading pane no longer shifts sideways** when the scrollbar appears or disappears. Opening the settings dialog caused it every time; the same jump happened whenever a short document followed a long one.
- **The settings dialog no longer scrolls the page behind it**, and a click outside dismisses it, as Esc and the close button already did.

### 🛠️ Developer / Core

- **The size budget was raised deliberately, to 108/62 KB.** The panel is the largest single frontend change so far, and the guard exists to keep the frontend three dependency-free files — it is still far below what a bundler or framework would cost.
- **Still no new dependencies**: the panel is plain HTML, CSS and JavaScript, the preferences are the same `localStorage` keys the app already used, and the only new Rust is a version command and a flag for remembering the window.

---

## 📅 Recent Version History

- **v0.3.3**: Settings panel, two-button toolbar, scroll and shift fixes.
- **v0.3.2**: Pinned section title, image lightbox, remembered window size and position.
- **v0.3.1**: Tabs — several documents at once in one window.
- **v0.3.0**: Syntax highlighting for fenced code, reopen-the-last-document, print styles removed.
- **v0.2.0**: Progressive rendering for large files, section links, image sizing, find toggles, reading position.

---

**Full Changelog**: [CHANGELOG.md](https://github.com/n4od/.md-viewer/blob/main/CHANGELOG.md)