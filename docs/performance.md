# Performance notes (measured, not aspirational)


Reproduce everything below with:

```powershell
.\scripts\measure.ps1      # time + memory (Windows PowerShell)
cargo test --manifest-path src-tauri\Cargo.toml                   # 33 tests, 1 ignored
cargo test --release rendering_a_large_document_stays_fast -- --nocapture
```

## Time to a visible window

Across runs: **~0.9–1.2 s** to a fully painted window (the latest run took 1125 ms).
`scripts/measure.ps1` records the latest run, with the per-process breakdown, in
[PERFORMANCE.md](PERFORMANCE.md).

That is the *whole* startup: process creation, WebView2 environment, HTML/CSS/JS
load, Rust-side read + render, IPC, first paint, then the window becomes visible.
The part this app owns is small — the 8 KB `sample.md` renders in ~4 ms — so the
spread tracks the WebView2 runtime (cold vs warm), not the document.

Rendering cost, measured through the guard-rail test (release profile, i.e. the
same optimisation settings the shipped binary uses):

| Document | Markdown in | HTML out | Time |
| :-- | --: | --: | --: |
| Large synthetic document | 124 KiB | 357 KiB | **~94 ms** |
| `sample.md` | 8 KiB | ~21 KiB | ~4 ms |

The larger HTML and the higher time are syntax highlighting: the renderer wraps code
tokens in spans. The guard-rail test fails above 750 ms, so this is a regression
threshold rather than a note.

## Memory

| Component | Processes | Working set (sum) | Private bytes (sum) |
| :-- | --: | --: | --: |
| `mdviewer.exe` (the app) | 1 | **22–23 MB** | **~5 MB** |
| `msedgewebview2.exe` (browser, renderer, GPU, utility, crashpad) | 6 | 291–301 MB | ~138 MB |
| Total | 7 | 313–324 MB | ~143 MB |

Working set is summed across processes, so shared pages (the same DLLs and font
data mapped into every Chromium process) are counted several times; private
bytes are the better proxy for what the app really costs the system. The exact
numbers from the last run are in [PERFORMANCE.md](PERFORMANCE.md), per process;
regenerate them with `scripts/measure.ps1 -WriteReport`.

## About the `<100 ms` / `<30 MB` targets

Both are unreachable for any WebView2-based shell, and this build does not
pretend otherwise. WebView2 itself — spawning the browser process, the renderer,
the GPU process and the utility process, then launching a document — is the
dominant term in both numbers, and it is not under the app's control. What *is*
under the app's control has been kept minimal: a 4.4 MB single binary, no
bundler, no framework, no web fonts, no network requests at start, one process
for app logic, and a ~94 ms render for a 124 KiB document.

If those two budgets are hard requirements, the honest options are:

1. **Drop the webview.** A native immediate-mode or retained UI (`egui`, `iced`,
   `slint`) with a Markdown-to-widgets renderer starts in tens of milliseconds
   and stays under 30 MB — at the cost of HTML/CSS fidelity and the ability to
   render arbitrary raw HTML in documents.
2. **Keep the webview, change the target.** Accept roughly 0.9–1.2 s and ~143 MB
   private, and
   spend the effort on making the first frame feel instant, which is what the
   hidden-until-first-paint design does: there is no white flash and no empty
   frame, the document simply appears complete.
