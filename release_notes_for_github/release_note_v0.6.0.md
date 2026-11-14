# Release v0.6.0 — A Window Without a Frame

The window lost its OS title bar. The toolbar is the title bar now: one band of chrome
instead of two, about 30px more text at every window size, and a window whose edges belong
to the app rather than to Windows.

## 🚀 Key Improvements

### 🪟 The Window Is Frameless

- **The toolbar became the title bar.** `decorations` is off, and the 48px row that was already there now does the caption's job: it drags the window, maximizes on a double-click, and carries minimize / maximize-restore / close at its right edge, with Windows' own close-red hover.
- **The document's name came back to the bar.** With no caption above it, a single open document had nowhere to be named — the tab strip only appears at two. The name shows at the left while one document is open, and hands over to the tabs at two.
- **The maximize glyph follows the window, not the click.** `Win`+`↑`, a double-click on the bar and the button itself all keep the restore glyph honest, because the state is read back from the window.
- **Snapping and resizing are untouched.** The drag is still `WM_NCLBUTTONDOWN(HTCAPTION)`, and Tauri keeps `WS_CAPTION` and `WS_THICKFRAME` on the window rather than stripping them, so drag-to-edge snapping, `Win`+arrow and edge-resizing behave exactly as they did under a real caption.
- **About 30px more text**, at every window size: one 48px band where a ~32px caption used to be stacked on a 48px toolbar.

### 📜 The Reading Surface Scrolls

- **The scrollbar starts below the toolbar.** It used to run the full height of the window beside the bar, because the bar was a sticky element inside the page. The page no longer scrolls; the reading surface does.
- **The window buttons are flush with the window edge.** The bar spans the full width now, instead of stopping short of the scrollbar's gutter.
- **Nothing else moved.** Reading position, the pinned section bar, the progress bar, find-scroll and `J`/`K` measure the same things they did — only the surface they measure changed.

### 🛠️ Under the Hood

- **Four window permissions, no more.** Dragging the bar and the three buttons need `start-dragging`, `minimize`, `toggle-maximize` and `close`; `is-maximized` and `internal-toggle-maximize` were already in the default set. The webview still cannot do anything else.
- **The front-end suite grew from 76 checks to 91:** the drag region, the name at one document and its hand-over to the strip at two, each window button reaching the window API, the glyph following the window, and the controls staying `<button>`s — which is what keeps the band draggable.
- **The size budget moved 112/66 → 118/70 KB** to pay for the title bar. `src/` is 112.4 KB of 118 and `app.js` 66.5 KB of 70.
- **The toolbar is a solid colour.** It was translucent with a backdrop blur, which was worth having while the document scrolled under it; now that nothing does, the filter had nothing to blur.

### ⚠️ One Known Limitation

- **The Windows 11 snap-layouts flyout** does not appear when you hover the maximize button. The OS draws it around the caption button a custom title bar does not have; restoring it would need `HTMAXBUTTON` from `WM_NCHITTEST`. Development happens on Windows 10, so this is unverified rather than known-broken — snapping itself is unaffected.

---

## 📅 Recent Version History

- **v0.6.0**: A frameless window whose toolbar is the title bar, and a reading surface that scrolls.
- **v0.5.0**: One band of chrome, Ctrl+wheel zoom, and the heading `#` removed.
- **v0.4.0**: Keyboard-driven tabs, session restore, front-end tests and a fmt/clippy gate.
- **v0.3.4**: Markdown files get a document icon of their own.
- **v0.3.3**: Settings panel, two-button toolbar, scroll and shift fixes.
- **v0.3.2**: Pinned section title, image lightbox, remembered window size and position.

---

**Full Changelog**: [CHANGELOG.md](https://github.com/n4od/md-viewer/blob/main/CHANGELOG.md)