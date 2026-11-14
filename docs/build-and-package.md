# Build, package and test

This is the Windows path: building the app, packaging the NSIS installer, registering
the file association and verifying the result. Frontend work, documentation and the
Rust tests need none of it — see [CONTRIBUTING.md](../CONTRIBUTING.md) for the smaller
setups, including Linux.

PowerShell commands below are written to be run from the repository root. If your
machine's execution policy blocks local scripts, prefix them with
`powershell -ExecutionPolicy Bypass -File` once rather than in every command.

## Step 0 — Prerequisites (once)

| Requirement | Check |
| :-- | :-- |
| Rust ≥ 1.90 (MSVC toolchain) | `rustc --version` |
| VS Build Tools w/ "Desktop development with C++" | needed for `link.exe` |
| WebView2 runtime (any Windows 11, or installer from Microsoft) | present by default |
| Node ≥ 18 — *optional*, only for the Tauri CLI / bundling | `node --version` |

## Step 1 — Debug build

```powershell
cd src-tauri
cargo run -- "..\samples\sample.md"     # opens the sample; `--` separates cargo args
```

Frontend changes: `cargo run` uses the assets embedded at compile time, so
re-run it. For hot reload of `src/`, use the Tauri CLI instead:

```powershell
npm install
npx tauri dev -- -- ".\samples\sample.md"
```

## Step 2 — Release build (single portable exe)

```powershell
cd src-tauri
cargo build --release
# -> src-tauri\target\release\mdviewer.exe   (no console window, 4.3 MB)
```

Register the association against that exe (see step 4) or copy just the `.exe`
anywhere you like — it is self-contained apart from the WebView2 runtime.

## Command line flags

The binary is a GUI-subsystem app (no console window), so it re-attaches to the
parent console when there is one and prints there:

| Invocation | Effect | Exit |
| :-- | :-- | --: |
| *(nothing)* | Window, empty state | 0 |
| `mdviewer FILE.md` | Window, that document | 0 |
| `mdviewer --register-default` | Writes the per-user association keys for *this* executable and refreshes the shell; no window, no dialog | 0 / 1 |
| `mdviewer --version` (`-V`) | Prints `mdviewer <version>` | 0 |
| `mdviewer --help` (`-h`) | Prints usage | 0 |

```powershell
# scripted install: a Windows Sandbox .wsb logon command, or a CI smoke test
& "$env:LOCALAPPDATA\MD Viewer\mdviewer.exe" --register-default
if ($LASTEXITCODE -ne 0) { throw 'association registration failed' }
```

`--register-default` makes MD Viewer a *candidate* handler — Windows still keeps
the final default choice, so the user confirms once (step 5).

## Step 3 — Package an installer with the association

```powershell
npm install
npx tauri build            # NSIS installer in src-tauri\target\release\bundle\
```

or with the scripts in `package.json`:

```powershell
npm run bundle             # tauri build (installer + associations)
npm run build              # tauri build --no-bundle (exe only)
npm run test               # cargo test
```

Run the produced `MD Viewer_0.4.0_x64-setup.exe`, install, then double-click any
`.md` file. If Windows asks which app to use, pick **MD Viewer** and tick
*Always use this app*.

The installer is verified end to end: silent install, one `MDViewer.Markdown`
ProgId pointing at the installed exe, one named entry in the picker, double-click
launch, and an uninstall that leaves no registry keys, app data or install
directory behind. It has also been run inside Windows Sandbox (2026-11-11). The
download-bootstrapper branch only executes where the WebView2 runtime is absent,
so an image without it remains the one case worth exercising explicitly.

Unattended checks:

```powershell
& ".\src-tauri\target\release\bundle\nsis\MD Viewer_0.4.0_x64-setup.exe" /S
& "$env:LOCALAPPDATA\MD Viewer\uninstall.exe" /S
```

Installing and uninstalling replaces keys shared with the dev registration, and
the uninstall hook removes WebView2's data folder (`%LOCALAPPDATA%\mdviewer`,
which holds the theme and recent-files list). Re-run
`scripts/register-file-association.ps1 -SetDefault` afterwards if you develop
against the portable exe.

## Step 4 — Register the association for an unpackaged build

```powershell
# inspect first — the script never needs admin rights
.\scripts/register-file-association.ps1 `
    -ExePath .\src-tauri\target\release\mdviewer.exe -SetDefault

# what does the shell actually offer now? (calls SHAssocEnumHandlers)
.\scripts/show-open-with.ps1

# undo
.\scripts/unregister-file-association.ps1
```

What it writes (all under `HKCU\Software\Classes`, i.e. current user):

```
MDViewer.Markdown\DefaultIcon            -> "…\mdviewer.exe",0
MDViewer.Markdown\shell\open\command     -> "…\mdviewer.exe" "%1"
.md\OpenWithProgids\MDViewer.Markdown    (empty)      # shows up in "Open with"
Applications\mdviewer.exe\shell\open\command, SupportedTypes
```

The registration also sets `Applications\mdviewer.exe\FriendlyAppName` and
`ApplicationIcon`, seeds `Explorer\FileExts\<ext>\OpenWithList` (the picker's short
list) and adds an `ApplicationAssociationToasts` entry. Those are what make the
app appear **named** in the "Open with" list: the list is built from
`HKCR\Applications\<exe>`, and an entry without a friendly name is drawn blank —
Windows 11 usually drops those, which is why a ProgId-only registration can look
like it did nothing. Verify what the shell really offers with
`scripts/show-open-with.ps1`, which calls the same API the picker does:

```
Apps the shell offers for '.md' (this is the 'Open with' list):
  MD Viewer       recommended  C:\Tools\mdviewer.exe
  Visual Studio Code           ...
```

If the entry is still missing in Explorer, the menu was cached: close and reopen
Explorer, or sign out and back in. Note that `assoc` and `ftype` only read the
machine hive, so they stay silent for a per-user registration.

## Step 5 — Making MD Viewer the default, from inside the app

The empty state has a **Set as default for .md files** button (with a line above it
saying whether MD Viewer currently owns `.md`). Pressing it runs the
`make_default` command, which does three things:

1. **Registers this executable**, per user, for the Markdown extensions:
   `MDViewer.Markdown` (ProgId, icon, `shell\open\command`),
   `Applications\mdviewer.exe` with `FriendlyAppName`/`ApplicationIcon`,
   `.md …\OpenWithProgids`, the extension's `SupportedTypes`, Explorer's
   `OpenWithList` MRU (merged, not overwritten) and the
   `ApplicationAssociationToasts` entries. The path is taken from
   `std::env::current_exe()`, so a portable copy, a moved folder or a Sandbox
   location registers *itself*.
2. **Tells the shell** with `SHChangeNotify(SHCNE_ASSOCCHANGED, …)`.
3. **Opens the Windows picker** (`SHOpenWithDialog` with
   `OAIF_ALLOW_REGISTRATION | OAIF_REGISTER_EXT`) on the document on screen — or
   on a temporary `.md` if none is open — which is the "How do you want to open
   this file?" dialog with the *Always use this app* box.

Step 3 is not a workaround, it is the requirement: since Windows 10, the default
handler for a file type is a per-user `UserChoice` that an application may not
write. `IApplicationAssociationRegistration::SetAppAsDefault` returns
`E_ACCESSDENIED` for ordinary desktop apps, and hand-writing `UserChoice` is both
ignored and hash-checked. So the app registers everything it is allowed to
register and then asks the user, which is exactly how a browser or editor behaves
when it offers to become your default.

Verify it without clicking: `cargo test -- --ignored
association_registration_round_trip` writes the real keys and reads them back,
and `scripts/show-open-with.ps1` prints what the shell would offer.

### Windows Sandbox / portable copy

Because registration is now built in, the exe-only workflow works — with the two
caveats that always apply to a fresh Windows instance:

* Press **Set as default** inside the Sandbox (or run
  `scripts/register-file-association.ps1 -ExePath <sandbox path>`), then confirm
  once in the dialog. Nothing persists after the Sandbox closes unless you run
  that from a `.wsb` logon command.
* A clean image may lack the **WebView2 runtime**, which no portable exe can
  install. If the window opens but never paints, install the Evergreen runtime
  first, or use the NSIS installer, which pulls it in (`webviewInstallMode`).

## Step 6 — Verification checklist

| Check | How |
| :-- | :-- |
| Association | Double-click `samples\sample.md` in Explorer → the rendered document appears. |
| In-app default handler | Empty state → **Set as default for .md files** → the Windows "How do you want to open this file?" dialog appears listing MD Viewer; tick *Always use this app*, then the empty-state line switches to "MD Viewer is the default app for .md files." |
| In-app registration keys | After the button (dialog dismissed or not): `reg query HKCU\Software\Classes\Applications\mdviewer.exe /v FriendlyAppName` and `scripts/show-open-with.ps1` both mention MD Viewer. |
| Startup with an argument | `.\mdviewer.exe samples\sample.md` from a terminal. |
| Startup without one | Launch the exe → the empty state with **Open a Markdown file**. |
| Single instance | With the viewer open, double-click `samples\sample-notes.md` → the *running* window switches documents (no second window). |
| Dynamic open | `Ctrl+O`, and the *Recently opened* list. |
| Drag & drop | Drop any `.md` file onto the window → overlay, then the document. |
| Live reload | Edit `samples\sample.md` in another editor and save → content updates, scroll position kept, a toast confirms. |
| Relative links | In `samples\sample.md`, click *the other sample* → loads `sample-notes.md`; click the external CommonMark link → default browser opens. |
| Local images | The images in `samples\sample.md` render (asset protocol). They live in `samples\images\` and are referenced relatively: Rust grants the document's own folder and nothing outside it, so an image in a sibling folder is refused by design. |
| Theme | `Ctrl+Shift+L` cycles system/light/dark; flipping Windows' theme follows when set to *system*. |
| Find | `Ctrl+F`, type `markdown` → highlights and `n/N` counter; Enter jumps, Esc closes. |
| Outline | `Ctrl+B` opens the drawer, the current section is highlighted while scrolling. |
| Empty/error states | Open a file, then rename it on disk and press `F5` → an inline error, the app stays usable. |
| Tabs | With a document open, double-click a second `.md` → the tab strip appears; switching restores each document's scroll position and find query, and the `×` closes one. |
| Tab keyboard | `Ctrl+Tab` / `Ctrl+Shift+Tab` move between tabs (wrapping), left/right and Home/End move within the strip, `Ctrl+W` closes the reading document, and a middle-click closes the tab under the pointer. Focus stays on the tab you just moved to. |
| Session restore | Open two documents, quit, relaunch with the *Reopen the documents I had open* setting on (no file on the command line) → the same two tabs come back with the one you were reading active; a tab whose file was deleted is dropped. |
| Syntax highlighting | Open `samples\highlight-demo.md` → fenced code is coloured per language; the unknown-language and untagged fences stay plain. |
| Settings | The gear opens the panel. Theme, text size and reading width apply at once and survive a restart; Esc, the close button or a click outside dismisses it, and the page behind does not scroll while it is open. |
| Pinned section title | Scroll past a heading → the bar under the toolbar names the section you are in, agreeing with the outline highlight. While the heading is still on screen the bar stays out of the way, so it never repeats what you can already read. |
| Image lightbox | Click an image in `samples\sample.md` → it opens at full size over a dimmed backdrop; Esc or a click outside closes it. |
| Window geometry | Resize and move the window, close it, relaunch → it returns to the same size and place (`%LOCALAPPDATA%\mdviewer\window.json`, switched in Settings). |
| File icon | In Explorer a `.md` shows the document page icon, not the app tile. After installing, run the app once: the installer writes the app icon on the ProgId and the first launch corrects it. |
| Command line | `src-tauri\target\release\mdviewer.exe --version` prints `mdviewer 0.4.0`; `--register-default` exits 0 and points the `MDViewer.Markdown` ProgId at that executable. |
| Tests | `cd src-tauri; cargo test` → 33 pass, 1 ignored: argument parsing and CLI flags, decoding, rendering, sanitising, alignment, image sizing, syntax highlighting, window geometry, the embedded file icon, recent-file filtering, installer/config consistency and render speed, plus one opt-in registry test. |
| Front-end tests | `node scripts/dev-server.js`, then open <http://localhost:8123/preview/tests.html> → `RESULT: PASS 91/91`. CI runs the same page in headless Edge (falling back to Chrome) and fails on anything but `data-result="pass"`. |

## Front-end tests

The test runner is a page, not a package: `preview/tests.html` loads
`preview/harness.html` in an iframe and drives the *real* `app.js` inside it, so the
fixture, the fake Tauri bridge and the markup are reused instead of duplicated. There
is still no test framework to install, and nothing new for the size budget to carry.

Run it by hand:

```powershell
node scripts/dev-server.js
# then open http://localhost:8123/preview/tests.html
```

or headless, which is what CI does. Chrome and Edge are GUI-subsystem binaries, so
their `--dump-dom` output is not captured by a pipe — redirect it to a file:

```powershell
node scripts/dev-server.js                                   # leave this running
$profile = "$env:TEMP\mdviewer-test-profile"
$dom = "$env:TEMP\mdviewer-tests-dom.html"
Start-Process "$env:ProgramFiles\Google\Chrome\Application\chrome.exe" -ArgumentList @(
  "--headless=new", "--disable-gpu", "--no-first-run", "--no-default-browser-check",
  "--disable-logging", "--log-level=3", "--user-data-dir=`"$profile`"",
  "--virtual-time-budget=30000", "--dump-dom",
  "http://127.0.0.1:8123/preview/tests.html"
) -RedirectStandardOutput $dom -RedirectStandardError "$dom.err" -Wait
[regex]::Match((Get-Content $dom -Raw), 'RESULT: (PASS|FAIL) \d+/\d+').Value
Get-Content $dom -Raw | Select-String 'data-result="pass"'
```

Use `127.0.0.1`, not `localhost`: the dev server binds IPv4 only. `ci.yml` does the
same with a 60-second budget, trying Edge first and falling back to Chrome.

The 91 checks cover the render path (heading ids, outline parity, table wrappers, copy
buttons, every token class, rewritten image sources), the pinned title, the lightbox,
tabs (in one toolbar row), the title bar and its window buttons, the settings dialog
including its scroll lock, per-document reading position, and progressive rendering
across the 1 MiB threshold.

Two notes for whoever adds a check:

- **Wait in microtasks, not timers.** A background or occluded tab throttles
  `setTimeout` and `requestAnimationFrame`, and the fake IPC resolves promises rather
  than posting messages, so `await tick()` is both correct and deterministic here. The
  one exception is `dialog.close()`, whose `close` event is a *task*, so the scroll-lock
  check awaits that event before asserting.
- **Clear `mdviewer.*` from `localStorage` first.** The page shares a profile with the
  harness, so without that a previous run's theme or resume setting leaks in.
- **Mark the stage you are entering.** A stalled `await` cannot be caught, so the suite
  fails after 20 seconds of idle time and names the stage in `data-stage`; CI prints
  that instead of a bare "running". Virtual time only advances while the page is idle,
  so the watchdog never fires just because the checks are slow.

## Troubleshooting

| Symptom | Cause / fix |
| :-- | :-- |
| Not listed under *Open with* | The app must have `HKCR\Applications\mdviewer.exe` with `FriendlyAppName`; run `scripts/show-open-with.ps1` to see the picker's own list. If it is listed there but not in Explorer, reopen Explorer (cached menu). |
| Double-click opens the old handler | Windows keeps a per-user `UserChoice`, which beats every registry default. Use *Open with → Choose another app → MD Viewer → Always*, or right-click a `.md` → *Open with → MD Viewer* for a one-off. |
| Nothing happens on double-click | Confirm the key exists: `reg query HKCU\Software\Classes\MDViewer.Markdown\shell\open\command`. |
| `assoc .md` / `ftype` report nothing | Both only read the machine hive; a per-user registration is invisible to them by design. |
| White flash on start | You launched a debug build from a terminal that was already open, or the watchdog (3 s) showed the window before the first paint. |
| Images missing | The image lives outside the document's folder — only that folder tree is scoped. |
| A huge file shows only the beginning | Truncation ceiling is 16 MB; the toast and the header say so. |
| Scripts rejected by execution policy | That is a local PowerShell setting, not a project requirement: run `powershell -ExecutionPolicy Bypass -File .\scripts\<name>.ps1`, or `Set-ExecutionPolicy -Scope Process Bypass` in that session. |
