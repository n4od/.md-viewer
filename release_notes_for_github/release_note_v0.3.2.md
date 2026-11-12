# Release v0.3.2 — Pinned Section, Image Zoom & Window Memory

Three small things: the section you are reading stays visible, screenshots open at
full size, and the window reopens exactly where you left it.

## 🚀 Key Improvements

### 🎨 Reading

- **The current section stays in view**: a slim bar under the toolbar shows the section you are in, updated as you scroll. In a long document you no longer have to scroll back up to remember where you are, and it shows the document name until a section has been picked.
- **Click an image to see it full size**: images in a note are capped at the article's width, which makes a screenshot unreadable. Clicking one now opens it over a dimmed backdrop. Esc, the close button or a click on the backdrop dismisses it, and clicking an image that is also a link opens the picture rather than navigating away.
- **Anchors still land correctly**: the pinned bar is part of the stack docked below the toolbar, alongside tabs, and the scroll offsets account for both — so a heading you jump to is never hidden underneath them.

### 🛠️ Windows and Core

- **The window reopens where you left it**: size and position are saved when the window closes and restored on the next launch, so the app stops snapping back to its default box. Stored in logical pixels, so a change of display scaling does not resize it.
- **No stranded windows**: a saved position that no longer lands on a connected monitor is ignored rather than honoured, so unplugging a display cannot leave the window off-screen. The size is still applied.
- **Nothing extra to install**: the geometry lives in a small `window.json` next to the WebView2 profile, which the uninstaller already removes.
- **A deliberate size-budget raise**: the frontend ceiling moves from 88/52 KB to 96/56 KB, because these three features took it past the old one. That guard exists to keep the frontend three dependency-free files; it is still far below what a bundler or framework would cost.

---

## 📅 Recent Version History

- **v0.3.2**: Pinned section title, image lightbox, remembered window size and position.
- **v0.3.1**: Tabs — several documents at once in one window.
- **v0.3.0**: Syntax highlighting for fenced code, reopen-the-last-document, print styles removed.
- **v0.2.0**: Progressive rendering for large files, section links, image sizing, find toggles, reading position.
- **v0.1.1**: Retry on a failed reload, recent-list housekeeping, `--version` / `--help` / `--register-default` flags.
- **v0.1.0**: First release — a read-only, single-window Markdown viewer with Windows file association.

---

**Full Changelog**: [CHANGELOG.md](https://github.com/n4od/md-viewer/blob/main/CHANGELOG.md)