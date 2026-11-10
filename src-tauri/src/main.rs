// MD Viewer — Rust backend.
//
// Responsibilities:
//   * capture the file path passed by Windows when a .md file is double-clicked
//   * read + render Markdown to HTML with pulldown-cmark, then sanitise it
//   * own IPC with the webview (commands + events)
//   * watch the open file and push live reloads
//   * hand the webview read access to the document's folder for local images
//
// Release builds are GUI-only: no console window flashes on startup.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, UNIX_EPOCH};

use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use pulldown_cmark::{html, Options, Parser};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Files larger than this are truncated before rendering so a stray huge file
/// cannot stall the UI.
const MAX_BYTES: u64 = 16 * 1024 * 1024;

/// Extensions we happily render as Markdown. Everything else is handed to the OS.
const MARKDOWN_EXT: &[&str] = &[
    "md", "markdown", "mdown", "mkd", "mkdn", "mdwn", "mdtxt", "mdtext", "txt",
];

const EVENT_OPEN: &str = "md://open";
const EVENT_RELOAD: &str = "md://reload";
const EVENT_ERROR: &str = "md://error";
const EVENT_NOTICE: &str = "md://notice";
const EVENT_DRAG: &str = "md://drag";

// ---------------------------------------------------------------------------
// IPC payloads
// ---------------------------------------------------------------------------

/// One rendered document, exactly what the frontend needs to paint.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Document {
    path: String,
    name: String,
    dir: String,
    html: String,
    size: u64,
    modified: Option<u64>,
    words: usize,
    lines: usize,
    truncated: bool,
    error: Option<String>,
}

impl Document {
    fn error(path: &Path, message: impl Into<String>) -> Self {
        Self {
            path: path.display().to_string(),
            name: file_name(path),
            dir: parent_dir(path),
            html: String::new(),
            size: 0,
            modified: None,
            words: 0,
            lines: 0,
            truncated: false,
            error: Some(message.into()),
        }
    }
}

/// Result of resolving a clicked link in the rendered document.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LinkOutcome {
    /// "document" | "external" | "missing" | "ignored"
    kind: &'static str,
    url: Option<String>,
    doc: Option<Document>,
}

impl LinkOutcome {
    fn ignored() -> Self {
        Self { kind: "ignored", url: None, doc: None }
    }
    fn missing(url: String) -> Self {
        Self { kind: "missing", url: Some(url), doc: None }
    }
    fn external(url: String) -> Self {
        Self { kind: "external", url: Some(url), doc: None }
    }
    fn document(url: String, doc: Document) -> Self {
        Self { kind: "document", url: Some(url), doc: Some(doc) }
    }
}

// ---------------------------------------------------------------------------
// App state
// ---------------------------------------------------------------------------

#[derive(Default)]
struct Viewer {
    /// Rendered once during setup, before the webview exists, so the first paint
    /// already has content.
    initial: Mutex<Option<Document>>,
    /// The document currently on screen; the file watcher uses it to discard
    /// events for files the user has already navigated away from.
    current: Mutex<Option<PathBuf>>,
    /// Held only to keep the watcher alive. Dropping it stops the watch thread.
    watcher: Mutex<Option<RecommendedWatcher>>,
    /// Bumped on every open; stale watch threads notice and exit.
    generation: AtomicU64,
}

// ---------------------------------------------------------------------------
// Path helpers
// ---------------------------------------------------------------------------

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

fn parent_dir(path: &Path) -> String {
    path.parent()
        .map(|dir| dir.display().to_string())
        .unwrap_or_default()
}

fn is_markdown(path: &Path) -> bool {
    path.extension()
        .map(|ext| {
            let ext = ext.to_string_lossy().to_ascii_lowercase();
            MARKDOWN_EXT.contains(&ext.as_str())
        })
        .unwrap_or(false)
}

/// Make a path absolute without the `\\?\` verbatim prefix that
/// `canonicalize` adds, so paths stay readable in the UI and in the asset scope.
fn absolutize(path: &Path) -> PathBuf {
    let joined = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    };
    let text = joined.to_string_lossy();
    match text.strip_prefix(r"\\?\") {
        Some(rest) => PathBuf::from(rest),
        None => PathBuf::from(text.into_owned()),
    }
}

/// `file:///C:/notes/todo.md` -> `C:/notes/todo.md`
fn strip_file_url(raw: &str) -> String {
    let lowered = raw.to_ascii_lowercase();
    let rest = if let Some(rest) = lowered.strip_prefix("file://") {
        &raw[raw.len() - rest.len()..]
    } else {
        return raw.to_string();
    };
    let decoded = percent_decode(rest.trim_start_matches('/'));
    // "C:/..." stays as is; "/C:/..." was already trimmed above.
    decoded
}

fn percent_decode(raw: &str) -> String {
    percent_encoding::percent_decode_str(raw)
        .decode_utf8_lossy()
        .into_owned()
}

fn paths_equal(a: &Path, b: &Path) -> bool {
    if a == b {
        return true;
    }
    if let (Ok(left), Ok(right)) = (a.canonicalize(), b.canonicalize()) {
        return left == right;
    }
    a.to_string_lossy()
        .eq_ignore_ascii_case(&b.to_string_lossy())
}

/// First plausible file argument on the command line.
///
/// Windows passes the double-clicked path as a plain argument, but a launch may
/// also carry switches, so flags are skipped and `--` ends flag parsing.
fn parse_cli_path(args: &[String]) -> Option<PathBuf> {
    let mut fallback: Option<PathBuf> = None;
    let mut after_dashdash = false;

    for raw in args.iter().skip(1) {
        let arg = raw.trim().trim_matches('"');
        if arg.is_empty() {
            continue;
        }
        if arg == "--" {
            after_dashdash = true;
            continue;
        }
        if !after_dashdash && arg.starts_with('-') {
            continue;
        }
        let path = absolutize(Path::new(&strip_file_url(arg)));
        if !path.is_file() {
            continue;
        }
        if is_markdown(&path) {
            return Some(path);
        }
        fallback.get_or_insert(path);
    }

    fallback
}
// ---------------------------------------------------------------------------
// Reading
// ---------------------------------------------------------------------------

/// Decode file bytes, honouring the UTF-8 / UTF-16 byte order marks that
/// Windows editors still emit. Invalid sequences degrade to U+FFFD instead of
/// failing, so a slightly broken file is still readable.
fn decode_text(bytes: &[u8]) -> String {
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8_lossy(&bytes[3..]).into_owned();
    }
    if bytes.starts_with(&[0xFF, 0xFE]) {
        let units = bytes[2..]
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect::<Vec<u16>>();
        return String::from_utf16_lossy(&units);
    }
    if bytes.starts_with(&[0xFE, 0xFF]) {
        let units = bytes[2..]
            .chunks_exact(2)
            .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
            .collect::<Vec<u16>>();
        return String::from_utf16_lossy(&units);
    }
    String::from_utf8_lossy(bytes).into_owned()
}

fn io_message(error: std::io::Error) -> String {
    match error.kind() {
        std::io::ErrorKind::NotFound => "File not found.".to_string(),
        std::io::ErrorKind::PermissionDenied => "Permission denied.".to_string(),
        _ => error.to_string(),
    }
}

/// Read at most [`MAX_BYTES`] and report whether the file was truncated.
fn read_source(path: &Path) -> Result<(String, bool), String> {
    let file = File::open(path).map_err(io_message)?;
    let metadata = file.metadata().map_err(io_message)?;
    if metadata.is_dir() {
        return Err("That path is a folder, not a file.".to_string());
    }

    let mut buffer = Vec::with_capacity(metadata.len().min(MAX_BYTES) as usize);
    let mut limited = (&file).take(MAX_BYTES + 1);
    limited
        .read_to_end(&mut buffer)
        .map_err(|error| format!("Could not read file: {error}"))?;

    let truncated = buffer.len() as u64 > MAX_BYTES;
    if truncated {
        buffer.truncate(MAX_BYTES as usize);
    }
    Ok((decode_text(&buffer), truncated))
}

fn count_words(source: &str) -> usize {
    source.split_whitespace().filter(|token| !token.is_empty()).count()
}

// ---------------------------------------------------------------------------
// Markdown -> HTML
// ---------------------------------------------------------------------------

fn markdown_options() -> Options {
    // GitHub-flavoured, minus smart punctuation (it rewrites quotes and dashes
    // in prose, which surprises people reading their own notes).
    Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_HEADING_ATTRIBUTES
        | Options::ENABLE_GFM
        | Options::ENABLE_DEFINITION_LIST
        | Options::ENABLE_SUPERSCRIPT
        | Options::ENABLE_SUBSCRIPT
        | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS
        | Options::ENABLE_PLUSES_DELIMITED_METADATA_BLOCKS
}

/// pulldown-cmark marks aligned table cells with an inline `style`. The app's
/// CSP is `style-src 'self'`, so inline style attributes never render; convert
/// the three possible values into classes *before* sanitising, which also lets
/// the allow-list drop `style` entirely.
fn alignment_to_classes(html: String) -> String {
    html.replace(" style=\"text-align: left\"", " class=\"align-left\"")
        .replace(" style=\"text-align: center\"", " class=\"align-center\"")
        .replace(" style=\"text-align: right\"", " class=\"align-right\"")
        .replace(" style=\"text-align:left\"", " class=\"align-left\"")
        .replace(" style=\"text-align:center\"", " class=\"align-center\"")
        .replace(" style=\"text-align:right\"", " class=\"align-right\"")
}

/// Raw HTML inside Markdown is a real XSS vector: this webview can call our IPC
/// commands. The document is parsed to HTML, then cleaned with an allow-list.
fn sanitize(dirty: &str) -> String {
    let mut builder = ammonia::Builder::default();

    // `id` keeps heading and footnote anchors working, `class` keeps the
    // renderer's semantics (alerts, code languages, task lists, alignment).
    builder.add_generic_attributes(["id", "class"]);

    // Extra elements that Markdown authors reasonably expect. No <script>,
    // <iframe>, <svg> or <form>: ammonia's defaults plus this list.
    builder.add_tags([
        "input",
        "section",
        "figure",
        "figcaption",
        "video",
        "audio",
        "source",
        "picture",
        "abbr",
        "time",
        "kbd",
        "mark",
    ]);
    builder.add_tag_attributes("input", ["type", "checked", "disabled"]);
    builder.add_tag_attributes("source", ["src", "srcset", "type", "media"]);
    builder.add_tag_attributes(
        "video",
        ["src", "poster", "controls", "loop", "muted", "preload", "width", "height"],
    );
    builder.add_tag_attributes("audio", ["src", "controls", "loop", "muted", "preload"]);
    builder.add_tag_attributes("time", ["datetime"]);
    builder.link_rel(Some("noopener noreferrer"));

    // Note: only the schemes ammonia allows by default (http/https/mailto/...)
    // and relative URLs survive; javascript:, vbscript: and data: links are
    // stripped before the HTML ever reaches the webview.
    builder.clean(dirty).to_string()
}

fn render_markdown(source: &str) -> String {
    let parser = Parser::new_ext(source, markdown_options());
    let mut rendered = String::with_capacity(source.len() + source.len() / 2 + 64);
    html::push_html(&mut rendered, parser);
    sanitize(&alignment_to_classes(rendered))
}

// ---------------------------------------------------------------------------
// Loading
// ---------------------------------------------------------------------------

fn load_document(path: &Path) -> Document {
    match read_source(path) {
        Ok((source, truncated)) => {
            let metadata = std::fs::metadata(path).ok();
            Document {
                path: path.display().to_string(),
                name: file_name(path),
                dir: parent_dir(path),
                html: render_markdown(&source),
                size: metadata.as_ref().map(|meta| meta.len()).unwrap_or(source.len() as u64),
                modified: metadata
                    .as_ref()
                    .and_then(|meta| meta.modified().ok())
                    .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                    .map(|since| since.as_millis() as u64),
                words: count_words(&source),
                lines: source.lines().count(),
                truncated,
                error: None,
            }
        }
        Err(message) => Document::error(path, message),
    }
}
// ---------------------------------------------------------------------------
// "Set as default" (per-user file association)
// ---------------------------------------------------------------------------

/// ProgId for Markdown documents. Kept in step with `bundle.fileAssociations` in
/// tauri.conf.json so the installer and the in-app action agree.
const PROG_ID: &str = "MDViewer.Markdown";
const APP_NAME: &str = "MD Viewer";

/// Extensions the in-app action claims. `.txt` is deliberately excluded: the
/// installer may offer it, but a viewer silently claiming plain-text files is
/// not a pleasant surprise.
const CLAIM_EXTENSIONS: &[&str] = &["md", "markdown", "mdown", "mkd", "mkdn", "mdwn", "mdtxt", "mdtext"];

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DefaultReport {
    progid: &'static str,
    executable: String,
    extensions: Vec<&'static str>,
    was_default: bool,
    registered: bool,
    picker_opened: bool,
    message: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DefaultStatus {
    progid: &'static str,
    executable: String,
    is_default: bool,
    supported: bool,
}

/// `"C:\path\mdviewer.exe" "%1"` — the command Explorer runs on a document.
fn open_command(exe: &Path) -> String {
    format!("\"{}\" \"%1\"", exe.display())
}

/// The executable actually running right now, so a portable or moved copy
/// registers *itself* rather than a path baked in at build time.
fn current_program() -> Result<PathBuf, String> {
    std::env::current_exe()
        .map(|exe| absolutize(&exe))
        .map_err(|error| format!("Cannot locate the running executable: {error}"))
}

fn is_default_progid(progid: Option<String>) -> bool {
    progid
        .map(|value| value.eq_ignore_ascii_case(PROG_ID))
        .unwrap_or(false)
}

/// The file the picker talks about: the document on screen when it is Markdown,
/// otherwise a throwaway `.md` so the dialog can still describe the extension.
fn association_target(app: &AppHandle) -> Option<PathBuf> {
    let state = app.state::<Viewer>();
    if let Some(path) = state.current.lock().unwrap().clone() {
        if is_markdown(&path) && path.is_file() {
            return Some(path);
        }
    }

    let mut path = std::env::temp_dir();
    path.push("mdviewer-set-as-default.md");
    if !path.exists() {
        std::fs::write(
            &path,
            "# MD Viewer\n\nThis file only exists so Windows can ask which app should open .md files.\n",
        )
        .ok()?;
    }
    Some(path)
}

/// Per-user association registration.
///
/// Windows owns file-type defaults: an app may register itself as a *candidate*
/// handler, but the default itself is the user's choice. So this module writes
/// everything that puts MD Viewer in the "Open with" list and then hands over to
/// the system dialog — nothing here tries to bypass that, because it cannot.
#[cfg(windows)]
mod shell_default {
    use super::*;
    use winreg::enums::{HKEY_CURRENT_USER, REG_NONE};
    use winreg::{RegKey, RegValue};
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::Shell::{
        SHChangeNotify, SHOpenWithDialog, OAIF_ALLOW_REGISTRATION, OAIF_REGISTER_EXT, OPENASINFO,
        SHCNE_ASSOCCHANGED, SHCNF_IDLIST,
    };

    fn io(error: std::io::Error) -> String {
        format!("Registry write failed: {error}")
    }

    /// Adds the executable to Explorer's per-extension "Open with" MRU without
    /// clobbering the entries already there (the MRU is a letter-keyed list whose
    /// order lives in `MRUList`, most recent first).
    fn seed_open_with_mru(root: &RegKey, dotted: &str, exe_name: &str) -> Result<(), String> {
        let path = format!(
            r"Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\{dotted}\OpenWithList"
        );
        let (key, _) = root.create_subkey(&path).map_err(io)?;

        let mut existing: Vec<(String, String)> = Vec::new();
        for entry in key.enum_values() {
            let (name, _) = entry.map_err(io)?;
            if name.eq_ignore_ascii_case("MRUList") {
                continue;
            }
            if let Ok(value) = key.get_value::<String, _>(&name) {
                if !value.is_empty() {
                    existing.push((name, value));
                }
            }
        }

        // Reuse our slot when re-registering, otherwise take the first free letter.
        let mut letter = existing
            .iter()
            .find(|(_, value)| value.eq_ignore_ascii_case(exe_name))
            .map(|(name, _)| name.clone());
        if letter.is_none() {
            letter = ('a'..='z')
                .map(|c| c.to_string())
                .find(|candidate| !existing.iter().any(|(name, _)| name == candidate));
        }
        let Some(letter) = letter else {
            return Ok(()); // 26 handlers already listed; the ProgId still works.
        };

        key.set_value(&letter, &exe_name.to_string()).map_err(io)?;

        let mut order = vec![letter];
        for previous in key.get_value::<String, _>("MRUList").unwrap_or_default().chars() {
            let name = previous.to_string();
            if !order.contains(&name) && existing.iter().any(|(value, _)| *value == name) {
                order.push(name);
            }
        }
        // Entries present in the key but missing from MRUList still belong there,
        // so a previously truncated order heals itself.
        let mut remaining: Vec<String> = existing.iter().map(|(name, _)| name.clone()).collect();
        remaining.sort();
        for name in remaining {
            if !order.contains(&name) {
                order.push(name);
            }
        }
        key.set_value("MRUList", &order.join("")).map_err(io)?;
        Ok(())
    }

    pub fn register(exe: &Path) -> Result<(), String> {
        let root = RegKey::predef(HKEY_CURRENT_USER);
        let command = open_command(exe);
        let icon = format!("\"{}\",0", exe.display());
        let exe_name = exe
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "mdviewer.exe".to_string());

        // ProgId: what the type is called, its icon, and how to open it.
        let (progid, _) = root
            .create_subkey(format!(r"Software\Classes\{PROG_ID}"))
            .map_err(io)?;
        progid.set_value("", &"Markdown Document".to_string()).map_err(io)?;

        let (progid_icon, _) = root
            .create_subkey(format!(r"Software\Classes\{PROG_ID}\DefaultIcon"))
            .map_err(io)?;
        progid_icon.set_value("", &icon).map_err(io)?;

        let (progid_command, _) = root
            .create_subkey(format!(r"Software\Classes\{PROG_ID}\shell\open\command"))
            .map_err(io)?;
        progid_command.set_value("", &command).map_err(io)?;

        // Applications\<exe> is the key the "Open with" list is built from, and
        // FriendlyAppName is the label it shows. Without it the entry is blank
        // (and Windows 11 tends to drop blank entries).
        let app_path = format!(r"Software\Classes\Applications\{exe_name}");
        let (app, _) = root.create_subkey(&app_path).map_err(io)?;
        app.set_value("", &APP_NAME.to_string()).map_err(io)?;
        app.set_value("FriendlyAppName", &APP_NAME.to_string()).map_err(io)?;
        app.set_value("ApplicationName", &APP_NAME.to_string()).map_err(io)?;
        app.set_value("ApplicationCompany", &APP_NAME.to_string()).map_err(io)?;
        app.set_value("ApplicationIcon", &icon).map_err(io)?;

        let (app_icon, _) = root.create_subkey(format!(r"{app_path}\DefaultIcon")).map_err(io)?;
        app_icon.set_value("", &icon).map_err(io)?;

        let (app_command, _) = root
            .create_subkey(format!(r"{app_path}\shell\open\command"))
            .map_err(io)?;
        app_command.set_value("", &command).map_err(io)?;

        let (supported, _) = root
            .create_subkey(format!(r"{app_path}\SupportedTypes"))
            .map_err(io)?;
        for ext in CLAIM_EXTENSIONS {
            supported.set_value(format!(".{ext}"), &String::new()).map_err(io)?;
        }

        for ext in CLAIM_EXTENSIONS {
            let dotted = format!(".{ext}");

            // Candidate handler for the extension (REG_NONE, empty payload).
            let (open_with, _) = root
                .create_subkey(format!(r"Software\Classes\{dotted}\OpenWithProgids"))
                .map_err(io)?;
            open_with
                .set_raw_value(
                    PROG_ID,
                    &RegValue { bytes: std::borrow::Cow::Borrowed(&[]), vtype: REG_NONE },
                )
                .map_err(io)?;

            // Explorer's short list (the "Open with" submenu) plus the toast
            // bookkeeping Windows consults. Without these the app only appears
            // under "Choose another app".
            seed_open_with_mru(&root, &dotted, &exe_name)?;
        }

        let (toasts, _) = root
            .create_subkey(r"Software\Microsoft\Windows\CurrentVersion\ApplicationAssociationToasts")
            .map_err(io)?;
        for ext in CLAIM_EXTENSIONS {
            toasts.set_value(format!("{exe_name}_{ext}"), &0u32).map_err(io)?;
        }

        Ok(())
    }

    /// The user's chosen handler for an extension, if they have ever chosen one.
    pub fn current_default(ext: &str) -> Option<String> {
        let root = RegKey::predef(HKEY_CURRENT_USER);
        root.open_subkey(format!(
            r"Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\.{ext}\UserChoice"
        ))
        .ok()?
        .get_value::<String, _>("ProgId")
        .ok()
    }

    /// Tells the shell that associations changed, so menus and icons refresh.
    pub fn notify_shell() {
        unsafe { SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, None, None) };
    }

    /// UTF-16, NUL-terminated, for the Win32 call.
    fn wide_null(value: &str) -> Vec<u16> {
        let mut wide: Vec<u16> = value.encode_utf16().collect();
        wide.push(0);
        wide
    }

    /// "How do you want to open this file?", with the "Always use this app"
    /// checkbox. This is the step Windows requires and reserves for the user.
    pub fn show_open_with(parent: isize, target: &Path) -> bool {
        let wide = wide_null(&target.as_os_str().to_string_lossy());

        let info = OPENASINFO {
            pcszFile: PCWSTR(wide.as_ptr()),
            pcszClass: PCWSTR::null(),
            oaifInFlags: OAIF_ALLOW_REGISTRATION | OAIF_REGISTER_EXT,
        };

        let parent = if parent == 0 {
            None
        } else {
            Some(HWND(parent as *mut core::ffi::c_void))
        };

        unsafe { SHOpenWithDialog(parent, &info).is_ok() }
    }

    /// The flag combination the dialog is opened with, exposed so a test can
    /// pin it down: registration must be allowed (so MD Viewer is listed) and
    /// the extension must be registered (so the "Always" choice sticks).
    #[cfg(test)]
    pub fn open_with_flag_bits() -> i32 {
        (OAIF_ALLOW_REGISTRATION | OAIF_REGISTER_EXT).0
    }

    #[cfg(test)]
    pub fn wide_null_for_test(value: &str) -> Vec<u16> {
        wide_null(value)
    }
}

#[cfg(not(windows))]
mod shell_default {
    use super::*;

    pub fn register(_exe: &Path) -> Result<(), String> {
        Err("Registering file associations is only implemented for Windows.".to_string())
    }
    pub fn current_default(_ext: &str) -> Option<String> {
        None
    }
    pub fn notify_shell() {}
    pub fn show_open_with(_parent: isize, _target: &Path) -> bool {
        false
    }
}

/// Register MD Viewer as a candidate handler and hand over to the Windows
/// picker. Only the user can set a file-type default, so the app registers,
/// tells the shell, and then shows the dialog where that choice is made.
#[tauri::command]
async fn make_default(app: AppHandle) -> Result<DefaultReport, String> {
    let exe = current_program()?;
    let was_default = is_default_progid(shell_default::current_default("md"));

    shell_default::register(&exe)?;
    shell_default::notify_shell();

    // The dialog is modal, so it runs on the main thread once this returns; the
    // frontend gets its report immediately and shows it while the user chooses.
    let target = association_target(&app);
    let handle = app.clone();
    let picker_opened = app
        .run_on_main_thread(move || {
            if let Some(path) = target {
                let parent = handle
                    .get_webview_window("main")
                    .and_then(|window| window.hwnd().ok())
                    .map(|hwnd| hwnd.0 as isize)
                    .unwrap_or(0);
                if !shell_default::show_open_with(parent, &path) {
                    let _ = handle.emit(
                        EVENT_NOTICE,
                        Notice { message: "Windows did not show the picker; use Open with manually." },
                    );
                }
            }
        })
        .is_ok();

    Ok(DefaultReport {
        progid: PROG_ID,
        executable: exe.display().to_string(),
        extensions: CLAIM_EXTENSIONS.to_vec(),
        was_default,
        registered: true,
        picker_opened,
        message: if was_default {
            "MD Viewer is already the default for .md files.".to_string()
        } else {
            "Registered. In the Windows dialog pick MD Viewer and tick \"Always use this app\"."
                .to_string()
        },
    })
}

/// Used by the empty state to say whether MD Viewer currently owns `.md`.
#[tauri::command]
fn default_status() -> DefaultStatus {
    DefaultStatus {
        progid: PROG_ID,
        executable: current_program()
            .map(|exe| exe.display().to_string())
            .unwrap_or_default(),
        is_default: is_default_progid(shell_default::current_default("md")),
        supported: cfg!(windows),
    }
}

// ---------------------------------------------------------------------------
// Live reload
// ---------------------------------------------------------------------------

/// Watch the document's *folder* rather than the file itself: editors save
/// atomically by writing a temp file and renaming it, which silently drops a
/// watch registered on the original inode/path.
fn watch_file(app: &AppHandle, path: PathBuf) {
    let (sender, receiver) = std::sync::mpsc::channel();
    let mut watcher = match notify::recommended_watcher(move |event| {
        let _ = sender.send(event);
    }) {
        Ok(watcher) => watcher,
        Err(_) => return,
    };

    let Some(dir) = path.parent() else { return };
    if watcher.watch(dir, RecursiveMode::NonRecursive).is_err() {
        return;
    }

    let generation = {
        let state = app.state::<Viewer>();
        // Replacing the watcher drops the previous one, which disconnects its
        // channel and lets that thread finish.
        *state.watcher.lock().unwrap() = Some(watcher);
        state.generation.fetch_add(1, Ordering::SeqCst)
    };

    let app = app.clone();
    std::thread::spawn(move || {
        let debounce = Duration::from_millis(180);
        let idle = Duration::from_secs(3600);
        let mut dirty = false;

        loop {
            let wait = if dirty { debounce } else { idle };
            match receiver.recv_timeout(wait) {
                Ok(Ok(event)) => {
                    if event.paths.iter().any(|touched| paths_equal(touched, &path)) {
                        dirty = true;
                    }
                }
                Ok(Err(_)) => {}
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    if !dirty {
                        continue;
                    }
                    dirty = false;

                    let state = app.state::<Viewer>();
                    if state.generation.load(Ordering::SeqCst) != generation {
                        return; // another file is open now
                    }
                    let still_current = state
                        .current
                        .lock()
                        .unwrap()
                        .as_ref()
                        .map(|current| paths_equal(current, &path))
                        .unwrap_or(false);
                    if !still_current {
                        return;
                    }

                    let document = load_document(&path);
                    let _ = app.emit(EVENT_RELOAD, &document);
                    let _ = app.emit(EVENT_NOTICE, Notice { message: "Reloaded from disk" });
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return,
            }
        }
    });
}

#[derive(Clone, Serialize)]
struct Notice {
    message: &'static str,
}

// ---------------------------------------------------------------------------
// Opening
// ---------------------------------------------------------------------------

/// Load a file, remember it as the current document, grant the webview read
/// access to its folder (for local images) and start watching it.
fn prepare_document(app: &AppHandle, path: &Path) -> Document {
    let path = absolutize(path);
    let document = load_document(&path);

    if document.error.is_none() {
        let state = app.state::<Viewer>();
        *state.current.lock().unwrap() = Some(path.clone());

        // Scope: the document's own folder, recursively, so `./images/x.png`
        // resolves. Nothing outside that tree becomes readable.
        if let Some(dir) = path.parent() {
            let _ = app.asset_protocol_scope().allow_directory(dir, true);
        }

        if let Some(window) = app.get_webview_window("main") {
            let _ = window.set_title(&format!("{} — MD Viewer", document.name));
        }

        watch_file(app, path);
    }

    document
}

fn focus_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

/// Called by the frontend once the first document is painted. Showing the
/// window only now is what removes the white flash on cold start.
#[tauri::command]
fn ready(app: AppHandle) {
    focus_main(&app);
}

/// The document rendered during setup from the startup path argument, if any.
#[tauri::command]
fn initial_document(state: State<'_, Viewer>) -> Option<Document> {
    state.initial.lock().unwrap().clone()
}

#[tauri::command]
fn load_path(app: AppHandle, path: String) -> Document {
    prepare_document(&app, Path::new(&path))
}

#[tauri::command]
fn reload(state: State<'_, Viewer>) -> Option<Document> {
    let path = state.current.lock().unwrap().clone()?;
    Some(load_document(&path))
}

#[tauri::command]
async fn open_dialog(app: AppHandle) -> Result<Option<Document>, String> {
    use tauri_plugin_dialog::DialogExt;

    let handle = app.clone();
    let picked = tauri::async_runtime::spawn_blocking(move || {
        handle
            .dialog()
            .file()
            .set_title("Open Markdown file")
            .add_filter(
                "Markdown",
                &["md", "markdown", "mdown", "mkd", "mkdn", "mdwn", "mdtxt", "mdtext"],
            )
            .add_filter("Text", &["txt"])
            .blocking_pick_file()
    })
    .await
    .map_err(|error| error.to_string())?;

    let Some(file) = picked else {
        return Ok(None);
    };
    let path = file.into_path().map_err(|error| error.to_string())?;
    Ok(Some(prepare_document(&app, &path)))
}

/// Resolve a link click. External URLs are opened with the OS default browser;
/// relative links to other Markdown files are loaded in place.
#[tauri::command]
fn open_link(app: AppHandle, href: String) -> LinkOutcome {
    use tauri_plugin_opener::OpenerExt;

    let href = href.trim().to_string();
    if href.is_empty() || href.starts_with('#') {
        return LinkOutcome::ignored();
    }

    let scheme = href
        .split_once(':')
        .map(|(head, _)| head.to_ascii_lowercase())
        .filter(|head| head.len() > 1 && head.chars().all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c)));

    match scheme.as_deref() {
        Some("http") | Some("https") | Some("mailto") | Some("tel") => {
            if let Err(error) = app.opener().open_url(href.clone(), None::<&str>) {
                return LinkOutcome::missing(format!("{href} ({error})"));
            }
            return LinkOutcome::external(href);
        }
        Some("file") => {
            let local = absolutize(Path::new(&strip_file_url(&href)));
            return open_local(&app, &local, href);
        }
        Some(_) => {
            // javascript:, data:, vbscript: and friends never leave the app.
            return LinkOutcome::ignored();
        }
        None => {}
    }

    // Relative link: resolve against the folder of the document on screen.
    let base = {
        let state = app.state::<Viewer>();
        let current = state.current.lock().unwrap().clone();
        match current.and_then(|path| path.parent().map(Path::to_path_buf)) {
            Some(dir) => dir,
            None => return LinkOutcome::ignored(),
        }
    };

    let pathless = href.split(['#', '?']).next().unwrap_or(&href);
    let decoded = percent_decode(pathless);
    if decoded.is_empty() {
        return LinkOutcome::ignored();
    }

    let target = absolutize(&base.join(decoded.replace('/', std::path::MAIN_SEPARATOR_STR)));
    open_local(&app, &target, href)
}

fn open_local(app: &AppHandle, path: &Path, href: String) -> LinkOutcome {
    use tauri_plugin_opener::OpenerExt;

    if !path.exists() {
        return LinkOutcome::missing(href);
    }
    if is_markdown(path) {
        let document = prepare_document(app, path);
        return LinkOutcome::document(href, document);
    }
    match app.opener().open_path(path.display().to_string(), None::<&str>) {
        Ok(()) => LinkOutcome::external(href),
        Err(error) => LinkOutcome::missing(format!("{href} ({error})")),
    }
}
// ---------------------------------------------------------------------------
// Startup
// ---------------------------------------------------------------------------

#[derive(Clone, Serialize)]
struct DragState {
    active: bool,
}

fn run() {
    // Capture the path Explorer handed us before any window exists, so the
    // very first paint already shows the document.
    let startup_path = parse_cli_path(&std::env::args().collect::<Vec<_>>());

    tauri::Builder::default()
        // The single-instance plugin must be registered before anything else.
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            if let Some(path) = parse_cli_path(&args) {
                let document = prepare_document(app, &path);
                let _ = app.emit(EVENT_OPEN, &document);
            }
            focus_main(app);
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(Viewer::default())
        .invoke_handler(tauri::generate_handler![
            ready,
            initial_document,
            load_path,
            reload,
            open_dialog,
            open_link,
            make_default,
            default_status
        ])
        .setup(move |app| {
            let handle = app.handle().clone();

            if let Some(path) = startup_path.clone() {
                let document = prepare_document(&handle, &path);
                let state = handle.state::<Viewer>();
                *state.initial.lock().unwrap() = Some(document);
            }

            // Safety net: the window stays hidden until the frontend says it has
            // painted. If the webview never gets that far, show it anyway.
            let watchdog = handle.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_secs(3));
                if let Some(window) = watchdog.get_webview_window("main") {
                    if !window.is_visible().unwrap_or(true) {
                        let _ = window.show();
                    }
                }
            });

            Ok(())
        })
        // Drag & drop from Explorer. Drops are delivered as *webview* events in
        // Tauri v2, not window events.
        .on_webview_event(|webview, event| match event {
            tauri::WebviewEvent::DragDrop(tauri::DragDropEvent::Enter { .. })
            | tauri::WebviewEvent::DragDrop(tauri::DragDropEvent::Over { .. }) => {
                let _ = webview.emit(EVENT_DRAG, DragState { active: true });
            }
            tauri::WebviewEvent::DragDrop(tauri::DragDropEvent::Leave) => {
                let _ = webview.emit(EVENT_DRAG, DragState { active: false });
            }
            tauri::WebviewEvent::DragDrop(tauri::DragDropEvent::Drop { paths, .. }) => {
                let _ = webview.emit(EVENT_DRAG, DragState { active: false });
                let chosen = paths
                    .iter()
                    .find(|path| path.is_file() && is_markdown(path))
                    .or_else(|| paths.iter().find(|path| path.is_file()));

                match chosen {
                    Some(path) => {
                        let document = prepare_document(webview.app_handle(), path);
                        let _ = webview.emit(EVENT_OPEN, &document);
                    }
                    None => {
                        let _ = webview.emit(
                            EVENT_ERROR,
                            Notice { message: "Nothing to read: drop a file, not a folder." },
                        );
                    }
                }
            }
            _ => {}
        })
        .build(tauri::generate_context!())
        .expect("failed to build MD Viewer")
        .run(|_app, _event| {});
}

fn main() {
    run();
}

// ---------------------------------------------------------------------------
// Tests — the parts worth checking without a GUI: argument parsing, decoding
// and the Markdown sanitising pipeline.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn temp_file(name: &str, contents: &[u8]) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!("mdviewer-test-{}-{}", std::process::id(), name));
        let mut file = File::create(&path).expect("create temp file");
        file.write_all(contents).expect("write temp file");
        path
    }

    #[test]
    fn cli_ignores_flags_and_picks_markdown() {
        let markdown = temp_file("cli.md", b"# hi");
        let args = vec![
            "mdviewer.exe".to_string(),
            "--debug".to_string(),
            markdown.display().to_string(),
        ];
        assert_eq!(parse_cli_path(&args), Some(markdown.clone()));

        let quoted = format!("\"{}\"", markdown.display());
        let args = vec!["mdviewer.exe".to_string(), quoted];
        assert_eq!(parse_cli_path(&args), Some(markdown.clone()));

        let _ = std::fs::remove_file(markdown);
    }

    #[test]
    fn cli_prefers_markdown_over_other_files() {
        let other = temp_file("cli.txt.zzz", b"plain");
        let markdown = temp_file("cli-preferred.md", b"# hi");
        let args = vec![
            "mdviewer.exe".to_string(),
            other.display().to_string(),
            markdown.display().to_string(),
        ];
        assert_eq!(parse_cli_path(&args), Some(markdown.clone()));
        let _ = std::fs::remove_file(other);
        let _ = std::fs::remove_file(markdown);
    }

    #[test]
    fn cli_returns_none_without_arguments() {
        assert_eq!(parse_cli_path(&["mdviewer.exe".to_string()]), None);
        assert_eq!(parse_cli_path(&[]), None);
    }

    #[test]
    fn file_urls_are_unwrapped() {
        assert_eq!(strip_file_url("file:///C:/notes/a.md"), "C:/notes/a.md");
        assert_eq!(strip_file_url("file:///home/me/a%20b.md"), "home/me/a b.md");
        assert_eq!(strip_file_url(r"C:\notes\a.md"), r"C:\notes\a.md");
    }

    #[test]
    fn decoding_handles_boms() {
        assert_eq!(decode_text(b"\xEF\xBB\xBFhello"), "hello");

        let utf16le = [&[0xFF, 0xFE][..], &"hi".encode_utf16().flat_map(u16::to_le_bytes).collect::<Vec<u8>>()].concat();
        assert_eq!(decode_text(&utf16le), "hi");

        assert_eq!(decode_text("plain".as_bytes()), "plain");
    }

    #[test]
    fn reads_and_truncates() {
        let small = temp_file("small.md", b"# title\n\nbody text");
        let (source, truncated) = read_source(&small).expect("read");
        assert!(source.contains("title"));
        assert!(!truncated);
        let _ = std::fs::remove_file(small);

        assert!(read_source(Path::new("this-file-does-not-exist-42.md")).is_err());
    }

    #[test]
    fn rendering_covers_gfm_extensions() {
        let source = "\
---
title: hidden frontmatter
---

# Heading

| a | b |
| :- | -: |
| 1 | 2 |

- [x] done
- [ ] todo

~~struck~~ and [link](https://example.com) and `code`

> [!NOTE]
> alert text

footnote[^1]

[^1]: the note
";
        let html = render_markdown(source);

        // Heading ids are added in the webview (slugified) so the outline can
        // link to them; the renderer itself emits a plain <h1>.
        assert!(html.contains("<h1>Heading</h1>"), "heading: {html}");
        assert!(html.contains("<table>"), "table: {html}");
        assert!(html.contains("type=\"checkbox\" checked"), "task list: {html}");
        assert!(html.contains("<del>struck</del>"), "strikethrough: {html}");
        assert!(html.contains("href=\"https://example.com\""), "link: {html}");
        assert!(html.contains("markdown-alert-note"), "alert class: {html}");
        assert!(html.contains("footnote-definition"), "footnote: {html}");
        assert!(!html.contains("hidden frontmatter"), "frontmatter should be dropped");
    }

    #[test]
    fn sanitising_removes_active_content() {
        let html = render_markdown(
            "<script>steal()</script>\n\n<img src=x onerror=\"steal()\">\n\n[click](javascript:steal())\n\n<div style=\"position:fixed\" onclick=\"steal()\">x</div>",
        );

        assert!(!html.contains("<script"), "script tag: {html}");
        assert!(!html.contains("onerror"), "event handler: {html}");
        assert!(!html.contains("javascript:"), "javascript url: {html}");
        assert!(!html.contains("onclick"), "inline handler: {html}");
        assert!(!html.contains("position:fixed"), "unsafe style: {html}");
    }

    #[test]
    fn table_alignment_is_emitted_as_classes() {
        let html = render_markdown("| a | b |\n| :- | -: |\n| 1 | 2 |\n");
        // Inline styles are blocked by the app's CSP, so alignment is a class.
        assert!(html.contains("class=\"align-left\""), "left alignment: {html}");
        assert!(html.contains("class=\"align-right\""), "right alignment: {html}");
        assert!(!html.contains("style="), "no inline styles may survive: {html}");
    }

    #[test]
    fn inline_styles_from_raw_html_are_dropped() {
        let html = render_markdown("<div style=\"color:red;text-align:center\">x</div>");
        assert!(!html.contains("style="), "style attribute leaked: {html}");
        assert!(!html.contains("color:red"), "style value leaked: {html}");
    }

    #[test]
    fn metadata_and_counts() {
        let source = "one two\nthree";
        assert_eq!(count_words(source), 3);
        assert_eq!(count_words("   "), 0);
    }

    #[test]
    fn extension_check_is_case_insensitive() {
        assert!(is_markdown(Path::new(r"C:\notes\ReadMe.MD")));
        assert!(is_markdown(Path::new("/tmp/notes.markdown")));
        assert!(!is_markdown(Path::new(r"C:\notes\image.png")));
        assert!(!is_markdown(Path::new(r"C:\notes\noextension")));
    }

    #[test]
    fn error_documents_carry_a_message() {
        let document = load_document(Path::new("definitely-missing-9.md"));
        assert!(document.error.is_some());
        assert!(!document.path.is_empty());
        assert!(document.html.is_empty());
    }

    #[test]
    fn open_command_quotes_the_executable() {
        let command = open_command(Path::new(r"C:\Program Files\MD Viewer\mdviewer.exe"));
        assert_eq!(command, r#""C:\Program Files\MD Viewer\mdviewer.exe" "%1""#);
    }

    #[test]
    fn default_progid_matching_ignores_case() {
        assert!(is_default_progid(Some("mdviewer.markdown".to_string())));
        assert!(!is_default_progid(Some(r"Applications\notepad.exe".to_string())));
        assert!(!is_default_progid(None));
    }

    #[test]
    fn claimed_extensions_are_bare_and_renderable() {        assert!(CLAIM_EXTENSIONS.contains(&"md"));
        assert!(CLAIM_EXTENSIONS.iter().all(|ext| !ext.starts_with('.')));
        assert!(CLAIM_EXTENSIONS.iter().all(|ext| is_markdown(Path::new(&format!("file.{ext}")))));
        // A viewer should not claim plain text as its own type.
        assert!(!CLAIM_EXTENSIONS.contains(&"txt"));
    }

    #[test]
    fn installer_progid_matches_the_runtime_registration() {
        // Tauri's NSIS template uses fileAssociations[].name as the ProgId. If
        // that drifts from PROG_ID, the installer and the in-app action register
        // different keys and the "Open with" list shows the app twice.
        let config = include_str!("../tauri.conf.json");
        assert!(
            config.contains(format!("\"name\": \"{PROG_ID}\"").as_str()),
            "tauri.conf.json fileAssociations[].name must be {PROG_ID}"
        );
        for ext in CLAIM_EXTENSIONS {
            assert!(
                config.contains(format!("\"{ext}\"").as_str()),
                "extension {ext} is missing from tauri.conf.json"
            );
        }
        assert!(
            config.contains("\"installerHooks\": \"./windows/hooks.nsh\""),
            "the NSIS hooks that write Applications\\<exe> are not wired up"
        );

        let hooks = Path::new(env!("CARGO_MANIFEST_DIR")).join("windows/hooks.nsh");
        assert!(hooks.is_file(), "missing {}", hooks.display());
    }

    /// Guard rail for the "instant" claim: parse + sanitise a document far
    /// larger than a typical note and make sure it stays in the tens of
    /// milliseconds. Run with `cargo test -- --nocapture` to see the number.
    #[test]
    fn rendering_a_large_document_stays_fast() {
        let mut source = String::new();
        for index in 0..600 {
            source.push_str(&format!(
                "# Section {index}\n\n\
                 Prose with **bold**, *italic*, `code` and a [link](https://example.com).\n\n\
                 - first item\n- second item\n\n\
                 | left | right |\n| :-- | --: |\n| a | b |\n\n\
                 > a quote\n\n\
                 ```rust\nfn main() {{ let x = \"{index}\"; }}\n```\n\n"
            ));
        }

        let started = std::time::Instant::now();
        let html = render_markdown(&source);
        let elapsed = started.elapsed();
        println!(
            "rendered {} KiB of markdown into {} KiB of html in {:?}",
            source.len() / 1024,
            html.len() / 1024,
            elapsed
        );

        assert!(html.len() > source.len());
        assert!(
            elapsed < Duration::from_millis(750),
            "rendering got slow: {elapsed:?}"
        );
    }

    /// The picker call itself needs a click, so its inputs are pinned here: the
    /// flag bits and the NUL-terminated UTF-16 file path.
    #[cfg(windows)]
    #[test]
    fn open_with_dialog_arguments_are_correct() {
        assert_eq!(shell_default::open_with_flag_bits(), 3, "OAIF_ALLOW_REGISTRATION | OAIF_REGISTER_EXT");

        let wide = shell_default::wide_null_for_test(r"C:\notes\a.md");
        assert_eq!(wide.last(), Some(&0), "must be NUL terminated");
        assert_eq!(wide.len(), r"C:\notes\a.md".encode_utf16().count() + 1);
        assert_eq!(String::from_utf16(&wide[..wide.len() - 1]).unwrap(), r"C:\notes\a.md");
    }

    /// Exercises the real registration path against the current user's registry
    /// and reads the keys back. Ignored by default because it changes the user's
    /// system; run it deliberately:
    ///
    ///     cargo test -- --ignored association_registration_round_trip
    ///
    /// Undo afterwards with `scripts\unregister-file-association.ps1`.
    #[cfg(windows)]
    #[test]
    #[ignore = "writes to HKCU; run explicitly"]
    fn association_registration_round_trip() {
        use winreg::enums::HKEY_CURRENT_USER;
        use winreg::RegKey;

        let exe = current_program().expect("running executable");
        shell_default::register(&exe).expect("register");
        shell_default::notify_shell();

        // The test binary has its own file name, so derive the key from the
        // running executable rather than assuming "mdviewer.exe".
        let exe_name = exe.file_name().expect("file name").to_string_lossy().into_owned();

        let root = RegKey::predef(HKEY_CURRENT_USER);

        let command: String = root
            .open_subkey(format!(r"Software\Classes\{PROG_ID}\shell\open\command"))
            .expect("ProgId command key")
            .get_value("")
            .expect("command value");
        assert_eq!(command, open_command(&exe));

        let friendly: String = root
            .open_subkey(format!(r"Software\Classes\Applications\{exe_name}"))
            .expect("Applications key")
            .get_value("FriendlyAppName")
            .expect("FriendlyAppName");
        assert_eq!(friendly, APP_NAME);

        for ext in CLAIM_EXTENSIONS {
            let listed = root
                .open_subkey(format!(r"Software\Classes\.{ext}\OpenWithProgids"))
                .expect("OpenWithProgids key")
                .get_raw_value(PROG_ID)
                .is_ok();
            assert!(listed, "{ext} is missing ProgId {PROG_ID}");
        }

        // The status command must agree with what was just written.
        assert!(!default_status().is_default || shell_default::current_default("md").is_some());
    }
}
