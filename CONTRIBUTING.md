# Contributing

MD Viewer is a Windows desktop app, but most of the work does not need Windows. This
guide starts with the smallest possible setup and only mentions Rust or the Tauri CLI
once you actually need them.

## Before you start

- **Read [ROADMAP.md](ROADMAP.md) first.** It lists what is planned *and* what is
  deliberately not. Anything under *Not planned* has a reason attached; a pull request
  that implements one will be closed rather than debated.
- **Open an issue before writing anything larger than a typo.** It is cheaper to agree
  on the shape of a change than to review the wrong one.
- **The constraints are architectural.** Three static frontend files, no bundler or
  framework, no web fonts, no new frontend dependencies, a hard size budget enforced in
  CI, and read-only behaviour. A change that trades one of those away needs an explicit
  decision, not a patch.
- **For internals, read [AGENTS.md](AGENTS.md)** — it records the invariants that are
  easy to break by accident (heading ids are added in JavaScript, the sanitizer strips
  inline `style`, the window stays hidden until the frontend says it painted).

## What you can work on, and what it needs

| Work | You need | Works on |
| :-- | :-- | :-- |
| Documentation, this guide, README | a text editor | **any OS** |
| Frontend — `src/index.html`, `styles.css`, `app.js`, the preview harness | **Node only** — no `npm install`, no Rust | **any OS** |
| The preview test suite (`preview/tests.html`) | Node, and any modern browser | **any OS** |
| Rust renderer, IPC and their unit tests | Rust; on Linux also Tauri's system packages (below) | Windows verified; **Linux verified in CI**; macOS untested |
| Running the actual app window | Windows + WebView2 | **Windows only** |
| Installer, file association, `scripts/*.ps1` | Windows, PowerShell, NSIS | **Windows only** |

If you change something that only the Windows build can exercise, say so in the pull
request and describe how you tested it — that is better than leaving the reader to guess.

## Quick start: no Rust, no `npm install`

```bash
git clone https://github.com/n4od/md-viewer.git
cd md-viewer
node scripts/dev-server.js
```

Then open:

- <http://127.0.0.1:8123/preview/harness.html> — the app, with a fake Tauri bridge, so
  `src/*` can be edited and reloaded without building anything.
- <http://127.0.0.1:8123/preview/tests.html> — the 68-check behavioural suite, which
  drives the real `app.js` inside that harness.

That is the whole setup for frontend and docs work. The server has no dependencies and
reads `src/` from disk on every request.

## Working on the Rust side

```bash
cargo test --manifest-path src-tauri/Cargo.toml
cargo run --manifest-path src-tauri/Cargo.toml -- samples/sample.md
```

`cargo run` on Windows opens the real window. On Linux and macOS the same command needs
Tauri's system libraries first (Debian/Ubuntu shown; see the
[Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for other distributions):

```bash
sudo apt update
sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file \
  libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev
```

The Windows-only code is behind `#[cfg(windows)]` and the Windows crates are
`[target.'cfg(windows)'.dependencies]`, so the crate compiles elsewhere; CI runs
`cargo check --all-targets` and `cargo test` on Ubuntu to keep that true. The packaging
path is not portable: `tauri.conf.json` targets NSIS, so `npm run bundle` only makes
sense on Windows.

## Building the Windows app and installer

Needs Windows, the WebView2 runtime (preinstalled on Windows 11), Node ≥ 18 and Rust.

```powershell
npm install         # the Tauri CLI; not needed for frontend-only work
npm run dev         # tauri dev — serves src/ from disk
npm run bundle      # tauri build — portable exe + NSIS installer
```

Associating, unregistering and measuring are Windows-only maintenance scripts; the
walkthrough is in [docs/build-and-package.md](docs/build-and-package.md). You do not need
them to contribute code.

## What CI runs

A change is ready when all of this is green. Running it locally first is faster than
finding out on a runner:

| Job | Gates |
| :-- | :-- |
| `windows-latest` | frontend size budget (`src/` ≤ 112 KB, `app.js` ≤ 66 KB), the 68-check front-end suite in a headless browser, `cargo fmt --check`, `cargo check --all-targets`, `cargo clippy -- -D warnings`, `cargo test` |
| `ubuntu-latest` | the 68-check front-end suite in headless Chromium, then `cargo check --all-targets` and `cargo test` — so the browser suite and the Rust side are both verified off Windows |

If a change genuinely needs more frontend room, raise the budget in the same commit and
say why. That is how the current numbers were reached.

## Pull requests

- **One topic per pull request.** Unrelated reformatting makes a diff unreviewable;
  `rustfmt` is enforced, so formatting is not something to fix by hand.
- **Tests travel with the behaviour.** Rust changes get a unit test in
  `src-tauri/src/main.rs`; frontend behaviour gets a check in `preview/tests.html`.
- **Docs are part of the change.** The README is the front door, the depth lives in
  `docs/`, and user-visible changes get an entry under `## [Unreleased]` in
  [CHANGELOG.md](CHANGELOG.md).
- **Screenshots** for anything that changes what the app looks like.
- **Commit messages** in the imperative mood, with the *why* in the body rather than a
  restatement of the diff.
- **No new dependencies** without discussing the trade-off in an issue first.

Contributions are accepted under the project's [MIT licence](LICENSE).

## Reporting bugs, questions and security issues

- Bugs and feature requests: use the issue templates.
- Questions, ideas, and "how do I…": open a
  [discussion](https://github.com/n4od/md-viewer/discussions) rather than an issue —
  that keeps the tracker for things that are actionable.
- Security problems: **do not open a public issue**. Follow [SECURITY.md](SECURITY.md).

## Code of conduct

Participation is covered by the [Code of Conduct](CODE_OF_CONDUCT.md). Be decent,
assume good faith, and keep disagreement about the code about the code.