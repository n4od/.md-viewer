# File association


```jsonc
{
  "bundle": {
    "active": true,
    "targets": ["nsis"],
    "icon": [
      "icons/32x32.png",
      "icons/128x128.png",
      "icons/128x128@2x.png",
      "icons/icon.ico",
      "icons/icon.png"
    ],
    "fileAssociations": [
      {
        "ext": ["md", "markdown", "mdown", "mkd", "mkdn", "mdwn", "mdtxt", "mdtext"],
        "name": "MDViewer.Markdown",
        "description": "Markdown Document",
        "mimeType": "text/markdown",
        "role": "Viewer"
      },
      {
        "ext": ["txt"],
        "name": "MDViewer.Text",
        "description": "Plain Text Document",
        "mimeType": "text/plain",
        "role": "Viewer"
      }
    ],
    "windows": {
      "nsis": {
        "installMode": "currentUser",
        "displayLanguageSelector": false,
        "compression": "lzma",
        "installerHooks": "./windows/hooks.nsh"
      }
    }
  }
}
```

Notes that matter in practice:

* `ext` is the only required field. `description` becomes the **Type** column in
  Explorer, the icon comes from the executable, and Explorer will hand the file
  to the app as an ordinary command-line argument.
* Associations are written by the **installer** (`tauri build` → NSIS), not by
  `cargo run`. For unpackaged builds use the registration script in
  [step 4](build-and-package.md#step-4--register-the-association-for-an-unpackaged-build).
* Windows 10/11 protects the *default* handler per user (`UserChoice`). The
  installer and the script make MD Viewer appear under **Open with** and set the
  ProgId; the first double-click may still ask once — choose MD Viewer and tick
  *Always*.
* `installMode: "currentUser"` keeps the NSIS installer admin-free and its
  registry writes in `HKCU`, which is also what makes the association
  immediately visible without a reboot.
* **`name` is the ProgId on Windows.** Tauri's NSIS template passes
  `fileAssociations[].name` straight into `Software\Classes\<name>`, so it is
  `MDViewer.Markdown` here — the same value as `PROG_ID` in `main.rs`. Using a
  display string like `"Markdown Document"` makes the installer register a
  *second* ProgId next to the one the in-app action writes, and "Open with" then
  lists the app twice. A test pins this (`installer_progid_matches_the_runtime_registration`).
* **The template is incomplete for "Open with".** It writes the ProgId, icon,
  command, extension default and a backup of the previous default — but not
  `Applications\<exe>` (which supplies `FriendlyAppName`, i.e. the name shown in
  the picker) and not `OpenWithProgids` (the candidate list). `windows/hooks.nsh`
  adds both in `NSIS_HOOK_POSTINSTALL`, and removes them plus the `*_backup`
  values, a now-dead extension default and WebView2's data folder in
  `NSIS_HOOK_POSTUNINSTALL`.

Window and security configuration that supports the rest of the app:

```jsonc
{
  "app": {
    "withGlobalTauri": true,            // exposes window.__TAURI__ (no bundler)
    "windows": [
      {
        "label": "main",
        "title": "MD Viewer",
        "width": 900, "height": 780, "minWidth": 380, "minHeight": 300,
        "center": true,
        "visible": false,               // shown by the `ready` command after first paint
        "dragDropEnabled": true,
        "additionalBrowserArgs": "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection --disable-background-networking --disable-component-update --no-first-run --disable-default-apps"
      }
    ],
    "security": {
      "csp": "default-src 'self'; img-src 'self' asset: http://asset.localhost http: https: data:; style-src 'self'; script-src 'self'; connect-src ipc: http://ipc.localhost; object-src 'none'; base-uri 'none'; form-action 'none'",
      "assetProtocol": { "enable": true, "scope": [] }
    }
  }
}
```

`assetProtocol.scope` deliberately starts **empty**: the Rust side calls
`allow_directory(<folder of the open file>, recursive)` every time a document is
opened, so local images work and nothing else becomes readable.
