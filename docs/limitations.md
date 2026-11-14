# Deliberate limitations

* **Read-only.** It is a viewer: no editing, no saving, no clipboard-on-edit.
* **The window has no OS caption.** The toolbar is the title bar, so the things a
  caption provides are either rebuilt (drag, minimize, maximize/restore, close, the
  document's name) or gone: the Windows 11 *snap layouts* flyout does not appear when
  you hover the maximize button, and right-clicking the bar does not open the system
  menu. Snapping itself is unaffected — `Win`+arrow and dragging the window to a screen
  edge both work, because the drag is the same `HTCAPTION` drag the caption used, and
  `Alt`+`Space` still reaches the system menu. Development happens on Windows 10, so
  Windows 11 niceties around the caption (the flyout, rounded corners) are unverified
  here rather than known-broken.
* **Raw HTML is sanitised, not trusted.** `<details>`, `<kbd>`, `<sub>` and
  friends render; `<script>`, inline handlers, `javascript:` URLs and inline
  `style` attributes do not. Intentional; see
  [why the sanitizer is not optional](architecture.md#why-the-sanitizer-is-not-optional).
* **Remote images load over the network** (README badges). Point the CSP
  `img-src` at `'self' asset: http://asset.localhost data:` if you want a fully
  offline viewer.
* **One window, many documents.** Several documents open as tabs in the one window;
  multiple *windows* remain a non-goal, because each window is a whole WebView2
  process tree and the single-instance plugin guarantees there is only ever one.
* **Windows decides who owns a file type.** The in-app action registers MD Viewer
  as a `.md` handler and opens the system picker; the final *default* is a user
  choice Windows reserves for itself, so it cannot be set silently. See
  [step 5](build-and-package.md#step-5--making-md-viewer-the-default-from-inside-the-app).
* **The in-app action claims `.md` and friends, not `.txt`** — the installer offers
  `.txt`, but a viewer claiming plain text uninvited is not a nice surprise.
* **Truncation at 16 MB**, and the word count is a whitespace estimate over the
  source (code fences included).
* **Windows is the verified target.** The code paths used (asset protocol, single
  instance, file associations, BOM decoding) are cross-platform, but macOS/Linux
  bundling has not been exercised here.
