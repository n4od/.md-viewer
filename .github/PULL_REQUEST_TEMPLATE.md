# Pull request

## What and why

<!-- What changes, and what problem it solves. Link the issue: "Closes #12". -->

## How it was verified

<!-- Commands you ran, checks you added, screenshots for anything visual. -->

## Checklist

- [ ] `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` are clean
- [ ] `cargo test` passes (33 pass, 1 ignored) and covers the behaviour that changed
- [ ] Frontend behaviour has a check in `preview/tests.html` (68 today)
- [ ] The size budget still fits (`src/` ≤ 112 KB, `app.js` ≤ 66 KB); if it was raised, the reason is in the commit message
- [ ] Docs updated where behaviour or packaging changed (`docs/`, README, CHANGELOG)
- [ ] No unrelated reformatting in the diff (rustfmt is enforced; run it on the files you touched)
- [ ] Screenshots attached for user-visible UI changes

## Notes for the reviewer

<!-- Trade-offs, follow-ups, anything you are unsure about. -->