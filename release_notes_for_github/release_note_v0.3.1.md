# Release v0.3.1 — Tabs for Multiple Documents

More than one document can be open at once, as tabs in the same window. The strip
stays out of the way until you actually have a second document, and each tab
remembers where you were.

## 🚀 Key Improvements

### 🚀 Tabs

- **Several documents at once**: open, close and switch between them in the one window, instead of opening a second file replacing what was on screen.
- **The strip only appears when it earns its place**: with a single document there is no tab strip at all, so ordinary reading looks exactly as it did before. The second document is what brings it in.
- **Opening a file that is already open activates it**: double-click the same `.md` in Explorer twice and the second click brings its tab forward rather than opening a duplicate.
- **Each tab remembers where you were**: switching returns to that document's own scroll position and section, and its own find query travels with it — search for something in one tab, switch away, and it is still there when you come back.
- **Everything opens into a tab**: the recent list, document links, drag and drop, the open dialog and a second double-click all land in the strip.

### 🛠️ Developer / Core

- **No second window, no second WebView2 tree**: a window costs a whole WebView2 process tree (measured at ~320 MB working set), so multiple windows would multiply memory for the same feature. Tabs share the window and cost nothing beyond the documents themselves.
- **No new Rust state**: the renderer already tracked exactly one active document, and switching a tab is simply `load_path` — which re-points the file watcher, the asset scope and the window title. A background tab is re-read when you return to it, so it is never stale.
- **Still three static frontend files**: the whole feature is frontend state plus about 20 lines of CSS, and every existing render path — progressive rendering, section links, syntax highlighting, reading position — is untouched.

---

## 📅 Recent Version History

- **v0.3.1**: Tabs — several documents at once in one window.
- **v0.3.0**: Syntax highlighting for fenced code, reopen-the-last-document, print styles removed.
- **v0.2.0**: Progressive rendering for large files, section links, image sizing, find toggles, reading position.
- **v0.1.1**: Retry on a failed reload, recent-list housekeeping, `--version` / `--help` / `--register-default` flags.
- **v0.1.0**: First release — a read-only, single-window Markdown viewer with Windows file association.

---

**Full Changelog**: [CHANGELOG.md](https://github.com/n4od/md-viewer/blob/main/CHANGELOG.md)