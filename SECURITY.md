# Security policy

MD Viewer runs locally: it reads Markdown files you open, renders them in a WebView2
webview, and can then grant the document's folder to the asset protocol so local images
load. There is no server, no account and no telemetry. The interesting attack surface is
therefore the render path, the file/registry handling around it, and the installer.

## Supported versions

| Version | Supported |
| :-- | :-- |
| 0.4.x (latest release) | ✅ |
| anything older | ❌ — please reproduce on the latest release |

## Reporting a vulnerability

**Please do not open a public issue for a security problem.**

Use GitHub's private advisory form:
[Report a vulnerability](https://github.com/n4od/md-viewer/security/advisories/new).
It keeps the report private until a fix is ready, and it credits you if you want credit.

Useful in a report:

- the version (Settings → About, or `mdviewer.exe --version`), and the Windows build;
- the smallest document, file name or command line that triggers it — and whether it
  requires the user to open a hostile file or can happen unprompted;
- what an attacker gains: read access to a path, code execution in the webview, a
  registry value changed, and so on.

## Scope

In scope, roughly in order of how much I care:

- **Sanitizer or CSP bypass** — Markdown/HTML that escapes the allow-list and runs
  script, injects inline handlers or `style`, or reaches a `javascript:` URL.
- **Escaping the asset-protocol scope** — reading a file outside the folder of the
  document you opened, through a crafted relative image path, a symlink, or URL
  encoding.
- **Path handling** — `..` in a link, an unusual argv path, or a UNC/device path that
  makes the app read somewhere it should not, or makes the watcher follow a different
  file than the one displayed.
- **Registry and installer effects** — writing outside the keys the app owns, an
  uninstall that leaves or clobbers another application's association, or an NSIS hook
  that can be abused by a crafted install directory.
- **Single-instance forwarding** — a second launch whose argv is handed to the running
  window in a harmful way.

Out of scope:

- **Unsigned binaries.** The releases are not code-signed yet; SmartScreen warnings and
  "unknown publisher" prompts are a known, documented gap rather than a vulnerability.
- **Remote images and links.** A document may reference remote images and links; those
  are the document author's content, and the CSP and the external-browser handling are
  described in [docs/architecture.md](docs/architecture.md).
- **Vulnerabilities in dependencies** (Tauri, WebView2, pulldown-cmark, ammonia,
  notify). Report them upstream; a note here is welcome so the pin can be bumped.
- **The things the app says it does not do** — see
  [docs/limitations.md](docs/limitations.md). A reader that refuses to edit is not
  vulnerable for refusing.

## What to expect

This is a single-maintainer project, so: an acknowledgement as soon as I see the report,
an assessment of severity and a fix in a patch release for anything in scope. I will not
pursue legal action over good-faith research on your own machine, and I will ask before
publishing details. Please give a fix a reasonable window before disclosing, and please
do not test against machines or files you do not own.