---
title: MD Viewer sample
tags: [demo, markdown]
---

# MD Viewer sample

This file exists to exercise every rendering path in one pass. Double-click it
in Explorer (after registering the association) or drop it on the window.

![Application icon](../src-tauri/icons/icon.png)

## Text, emphasis and inline code

Regular prose with **bold**, *italic*, ***both***, ~~struck through~~, `inline code`,
a [link to the other sample](sample-notes.md), an [external link](https://commonmark.org),
a footnote reference[^why], H~2~O, and E = mc^2^.

[^why]: Footnotes keep their anchors, so this reference scrolls back here.

### Lists

* Unordered item
* Another item
  * Nested item
  * A second nested item
* Third item

1. Ordered item
2. Second item
   1. Nested ordered item
   2. Another
3. Third item

### Task list

- [x] Render CommonMark
- [x] Render the GitHub extensions
- [x] Live reload on disk changes
- [ ] Solve everything else

### Table with alignment

| Feature        | State | Notes                                  |
| :------------- | :---: | -------------------------------------: |
| Tables         | done  | alignment survives sanitising          |
| Footnotes      | done  | anchors and back-references work       |
| Local images   | done  | served through the asset protocol      |
| Remote images  | done  | badges load over https                 |

### Code

Inline `cargo build --release` and a fenced block with a language label:

```rust
fn parse_cli_path(args: &[String]) -> Option<PathBuf> {
    // Windows hands us the double-clicked file as a plain argv entry.
    args.iter().skip(1).map(PathBuf::from).find(|p| p.is_file())
}
```

```powershell
npm run tauri dev -- -- "samples\sample.md"
```

### Blockquotes and alerts

> A plain blockquote, indented and quiet.

> [!NOTE]
> Alerts are a GitHub extension. This one is informational.

> [!WARNING]
> This one warns about something.

> [!CAUTION]
> And this one is the loudest.

### Images and rules

Images resolve relative to this file's folder:

![Icon, again](../src-tauri/icons/32x32.png)

---

## Long section (for the scroll bar and the outline)

The paragraph below is deliberately repetitive so the progress bar, the smooth
scrolling and the outline scroll-spy have something to chew on. Jump around with
`J` and `K`, or press <kbd>Ctrl</kbd>+<kbd>B</kbd> to toggle the outline.

Markdown is a plain-text formatting syntax. Markdown is a plain-text formatting
syntax. Markdown is a plain-text formatting syntax. Markdown is a plain-text
formatting syntax. Markdown is a plain-text formatting syntax. Markdown is a
plain-text formatting syntax. Markdown is a plain-text formatting syntax.

### Keyboard shortcuts

| Shortcut                | Action                        |
| :---------------------- | :---------------------------- |
| <kbd>Ctrl</kbd>+<kbd>O</kbd> | Open a file              |
| <kbd>Ctrl</kbd>+<kbd>R</kbd> | Reload from disk         |
| <kbd>Ctrl</kbd>+<kbd>F</kbd> | Find in document         |
| <kbd>Ctrl</kbd>+<kbd>B</kbd> | Toggle the outline       |
| <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>L</kbd> | Cycle the theme |
| <kbd>Ctrl</kbd>+<kbd>+</kbd> / <kbd>-</kbd> / <kbd>0</kbd> | Reading size |
| <kbd>F5</kbd>           | Reload from disk              |
| <kbd>J</kbd> / <kbd>K</kbd> | Scroll down / up           |
| <kbd>Esc</kbd>          | Close find or the outline     |

### Definition list

Rust
  : A language with no runtime and no garbage collector.

Tauri
  : A shell that puts a system webview in a native window.

### Final heading

That is the whole tour. Edit this file in another editor and watch the viewer
reload itself.
