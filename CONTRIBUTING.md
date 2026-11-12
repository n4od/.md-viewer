# Contributing

Thanks for taking an interest. MD Viewer is maintained by one person, so a little
structure here saves time for both of us.

## Before you start

- **Read [ROADMAP.md](ROADMAP.md) first.** It lists what is planned *and* what is
  deliberately not. Anything under *Not planned* has a reason attached; a pull request
  that implements one will be closed rather than debated.
- **Open an issue before writing anything larger than a typo.** It is cheaper to agree
  on the shape of a change than to review the wrong one.
- **The constraints are architectural, not stylistic.** Three static frontend files, no
  bundler or framework, no web fonts, no new frontend dependencies, a hard size budget
  (`src/` ≤ 112 KB, `app.js` ≤ 66 KB) enforced in CI, and read-only behaviour. A change
  that trades one of those away needs an explicit decision, not a patch.
- **Windows is the verified target.** The Rust paths are largely portable, but
  association, installer and WebView2 behaviour are only exercised on Windows.

If you are changing internals, read [AGENTS.md](AGENTS.md) even though it is written for
coding agents: it records the invariants that are easy to break accidentally (heading
ids are added in JS, the sanitizer strips inline `style`, the window stays hidden until
the frontend says it painted, and so on).

## Getting set up

Rust **stable 1.90 or newer** (`rust-version` in `src-tauri/Cargo.toml`). The Tauri CLI
needs Node ≥ 18, but the app itself does not.

```powershell
cargo run --manifest-path src-tauri/Cargo.toml -- samples\sample.md
cargo test --manifest-path src-tauri/Cargo.toml
```

The frontend is three static files with no build step, so iterate on it in a browser:

```powershell
node scripts/dev-server.js
# http://localhost:8123/preview/harness.html    the app, with a fake Tauri bridge
# http://localhost:8123/preview/tests.html      68 behavioural checks
```

Building the installer needs the CLI:

```powershell
npm.cmd install          # npm.ps1 / npx.ps1 are blocked by execution policy on this machine
npx.cmd tauri build
```

## What CI runs

A change is merged when all of this is green. Running it locally first is faster than
finding out on a runner:

| Gate | Command |
| :-- | :-- |
| Formatter | `cargo fmt --check` |
| Linter | `cargo clippy --all-targets -- -D warnings` |
| Rust tests | `cargo test` — 33 pass, 1 ignored registry test |
| Frontend tests | `preview/tests.html` in a headless browser — 68 checks |
| Size budget | `src/` ≤ 112 KB and `app.js` ≤ 66 KB |

If a change genuinely needs more frontend room, raise the budget in the same commit and
say why. That is how the current numbers were reached.

## Pull requests

- **One topic per pull request.** Unrelated reformatting makes a diff unreviewable;
  `rustfmt` is enforced, so formatting is not something to fix by hand.
- **Tests travel with the behaviour.** Rust changes get a unit test in
  `src-tauri/src/main.rs`; frontend behaviour gets a check in `preview/tests.html`.
- **Docs are part of the change.** The README is the front door, the depth lives in
  `docs/`, and user-visible changes get a [CHANGELOG.md](CHANGELOG.md) entry.
- **Screenshots** for anything that changes what the app looks like.
- **Commit messages** in the imperative mood, with the *why* in the body rather than a
  restatement of the diff.
- **No new dependencies** without discussing the trade-off in an issue first.

Contributions are accepted under the project's [MIT licence](LICENSE).

## Reporting bugs and security issues

- Bugs and feature requests: use the issue templates.
- Security problems: **do not open a public issue**. Follow [SECURITY.md](SECURITY.md).

## Code of conduct

Participation is covered by the [Code of Conduct](CODE_OF_CONDUCT.md). Be decent,
assume good faith, and keep disagreement about the code about the code.