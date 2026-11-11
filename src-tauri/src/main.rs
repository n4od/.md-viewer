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
use serde::{Deserialize, Serialize};
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

/// Render, then give each local image its real pixel dimensions so the browser
/// reserves the right box before the file loads (no layout shift). `dir` is the
/// document's folder, used to resolve relative image paths.
fn render_markdown_in(source: &str, dir: Option<&Path>) -> String {
    let parser = Parser::new_ext(source, markdown_options());
    let mut rendered = String::with_capacity(source.len() + source.len() / 2 + 64);
    html::push_html(&mut rendered, parser);
    let sized = add_image_dimensions(rendered, dir);
    sanitize(&alignment_to_classes(highlight_code_blocks(sized)))
}

/// Add `width`/`height` to local `<img>` tags that carry neither, so the browser
/// can reserve the box from the intrinsic aspect ratio (the stylesheet pairs
/// `max-width: 100%` with `height: auto`, so this never distorts an image).
/// Author-supplied dimensions win, and anything that is not a plain local path
/// (http:, data:, root-absolute) is left untouched.
fn add_image_dimensions(html: String, dir: Option<&Path>) -> String {
    let dir = match dir {
        Some(dir) => dir,
        None => return html,
    };

    let mut out: Option<String> = None;
    let mut copied = 0usize;   // start of the region not yet copied to `out`
    let mut search = 0usize;
    // A repeated image (an icon used a hundred times) should be read once.
    let mut sizes: std::collections::HashMap<PathBuf, Option<(u32, u32)>> =
        std::collections::HashMap::new();

    while let Some(offset) = html[search..].find("<img") {
        let start = search + offset;
        let end = match tag_end(&html, start) {
            Some(end) => end,
            None => break,
        };
        search = end + 1;
        let tag = &html[start..=end];

        if attribute_value(tag, "width").is_some() || attribute_value(tag, "height").is_some() {
            continue;
        }
        let src = match attribute_value(tag, "src") {
            Some(src) => src,
            None => continue,
        };
        let size = local_image_path(&src, dir)
            .map(|path| *sizes.entry(path).or_insert_with_key(|path| image_dimensions(path)))
            .unwrap_or(None);
        let (width, height) = match size {
            Some(size) => size,
            None => continue,
        };

        // Insert before the tag's closing ">", stepping back over a self-closing
        // "/" so the attributes land before it, not after.
        let mut insert = end;
        while insert > start && matches!(html.as_bytes()[insert - 1], b' ' | b'/') {
            insert -= 1;
        }
        let buffer = out.get_or_insert_with(|| String::with_capacity(html.len() + 64));
        buffer.push_str(&html[copied..insert]);
        buffer.push_str(&format!(" width=\"{width}\" height=\"{height}\""));
        copied = insert;
    }

    match out {
        Some(mut buffer) => {
            buffer.push_str(&html[copied..]);
            buffer
        }
        None => html,
    }
}

/// Index of the `>` closing the tag opened at `start`, skipping any `>` that sits
/// inside a quoted attribute value.
fn tag_end(html: &str, start: usize) -> Option<usize> {
    let bytes = html.as_bytes();
    let mut quote = 0u8;
    let mut i = start;
    while i < bytes.len() {
        let byte = bytes[i];
        if quote != 0 {
            if byte == quote {
                quote = 0;
            }
        } else if byte == b'"' || byte == b'\'' {
            quote = byte;
        } else if byte == b'>' {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// Value of `name` in a single tag, with HTML entities decoded. The renderer
/// escapes `&` and `"`; values may be single-quoted, double-quoted or bare.
fn attribute_value(tag: &str, name: &str) -> Option<String> {
    let bytes = tag.as_bytes();
    let mut i = 1;   // past '<'
    while i < bytes.len() && !bytes[i].is_ascii_whitespace() && bytes[i] != b'>' && bytes[i] != b'/' {
        i += 1;      // tag name
    }
    while i < bytes.len() {
        while i < bytes.len() && (bytes[i].is_ascii_whitespace() || bytes[i] == b'/') {
            i += 1;
        }
        if i >= bytes.len() || bytes[i] == b'>' {
            break;
        }
        let name_start = i;
        while i < bytes.len()
            && !bytes[i].is_ascii_whitespace()
            && bytes[i] != b'='
            && bytes[i] != b'>'
            && bytes[i] != b'/'
        {
            i += 1;
        }
        let found = &tag[name_start..i];
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() || bytes[i] != b'=' {
            continue;   // boolean attribute
        }
        i += 1;
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        let value = if i < bytes.len() && (bytes[i] == b'"' || bytes[i] == b'\'') {
            let quote = bytes[i];
            i += 1;
            let value_start = i;
            while i < bytes.len() && bytes[i] != quote {
                i += 1;
            }
            let value = &tag[value_start..i];
            if i < bytes.len() {
                i += 1;
            }
            value
        } else {
            let value_start = i;
            while i < bytes.len() && !bytes[i].is_ascii_whitespace() && bytes[i] != b'>' {
                i += 1;
            }
            &tag[value_start..i]
        };
        if found.eq_ignore_ascii_case(name) {
            return Some(unescape_entities(value));
        }
    }
    None
}

fn unescape_entities(raw: &str) -> String {
    if !raw.contains('&') {
        return raw.to_string();
    }
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(index) = rest.find('&') {
        out.push_str(&rest[..index]);
        rest = &rest[index..];
        // One pass, so "&amp;quot;" stays the literal text it is.
        let (decoded, consumed) = if rest.starts_with("&amp;") {
            ("&", 5)
        } else if rest.starts_with("&quot;") {
            ("\"", 6)
        } else if rest.starts_with("&#39;") || rest.starts_with("&apos;") {
            ("'", if rest.starts_with("&#39;") { 5 } else { 6 })
        } else if rest.starts_with("&lt;") {
            ("<", 4)
        } else if rest.starts_with("&gt;") {
            (">", 4)
        } else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        out.push_str(decoded);
        rest = &rest[consumed..];
    }
    out.push_str(rest);
    out
}

/// A relative image src resolved against the document folder, or None when the
/// file is not there or the src is not a local path at all.
fn local_image_path(src: &str, dir: &Path) -> Option<PathBuf> {
    let src = src.split(['?', '#']).next().unwrap_or(src);
    if src.is_empty() || src.starts_with("//") || src.starts_with('/') || src.starts_with('\\') {
        return None;
    }
    if looks_like_scheme(src) {
        return None;
    }
    let decoded = percent_decode(src);
    let resolved = dir.join(Path::new(&decoded));
    if resolved.is_file() {
        Some(resolved)
    } else {
        None
    }
}

/// `http:`, `data:` and friends. A Windows path like `C:\notes\x.png` reads like
/// a scheme too, which matches how the frontend already treats it, so it is left
/// alone rather than guessed at.
fn looks_like_scheme(src: &str) -> bool {
    match src.as_bytes().first() {
        Some(byte) if byte.is_ascii_alphabetic() => {}
        _ => return false,
    }
    match src.find(':') {
        Some(colon) => {
            colon > 0
                && src[..colon]
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'+' || b == b'-' || b == b'.')
        }
        None => false,
    }
}

/// Pixel dimensions straight from the file header: no decoding, no dependency.
/// PNG, JPEG, GIF and WebP are covered; anything else returns None, which just
/// leaves that image without reserved space. Only the first 64 KiB are read.
fn image_dimensions(path: &Path) -> Option<(u32, u32)> {
    use std::io::Read;

    let mut head = Vec::new();
    std::fs::File::open(path)
        .ok()?
        .take(64 * 1024)
        .read_to_end(&mut head)
        .ok()?;

    if head.len() >= 24 && head.starts_with(b"\x89PNG\r\n\x1a\n") && &head[12..16] == b"IHDR" {
        return Some((
            u32::from_be_bytes([head[16], head[17], head[18], head[19]]),
            u32::from_be_bytes([head[20], head[21], head[22], head[23]]),
        ));
    }
    if head.len() >= 10 && (head.starts_with(b"GIF87a") || head.starts_with(b"GIF89a")) {
        return Some((
            u16::from_le_bytes([head[6], head[7]]) as u32,
            u16::from_le_bytes([head[8], head[9]]) as u32,
        ));
    }
    if head.len() >= 4 && head[0] == 0xff && head[1] == 0xd8 {
        return jpeg_dimensions(&head);
    }
    if head.len() >= 30 && head.starts_with(b"RIFF") && &head[8..12] == b"WEBP" {
        return webp_dimensions(&head);
    }
    None
}

fn jpeg_dimensions(head: &[u8]) -> Option<(u32, u32)> {
    let mut i = 2;   // past the Start-Of-Image marker
    while i + 9 <= head.len() {
        if head[i] != 0xff {
            i += 1;
            continue;
        }
        let marker = head[i + 1];
        // Markers with no length field.
        if marker == 0xff || marker == 0x01 || (0xd0..=0xd9).contains(&marker) {
            i += 2;
            continue;
        }
        let length = u16::from_be_bytes([head[i + 2], head[i + 3]]) as usize;
        if length < 2 {
            return None;
        }
        // Start Of Frame (any variant except DHT/JPG/DAC).
        if (0xc0..=0xcf).contains(&marker) && marker != 0xc4 && marker != 0xc8 && marker != 0xcc {
            return Some((
                u16::from_be_bytes([head[i + 7], head[i + 8]]) as u32,
                u16::from_be_bytes([head[i + 5], head[i + 6]]) as u32,
            ));
        }
        i += 2 + length;
    }
    None
}

fn webp_dimensions(head: &[u8]) -> Option<(u32, u32)> {
    match &head[12..16] {
        b"VP8X" => Some((
            1 + u32::from_le_bytes([head[24], head[25], head[26], 0]),
            1 + u32::from_le_bytes([head[27], head[28], head[29], 0]),
        )),
        b"VP8L" => {
            let bits = u32::from_le_bytes([head[21], head[22], head[23], head[24]]);
            Some(((bits & 0x3fff) + 1, ((bits >> 14) & 0x3fff) + 1))
        }
        b"VP8 " => Some((
            (u16::from_le_bytes([head[26], head[27]]) & 0x3fff) as u32,
            (u16::from_le_bytes([head[28], head[29]]) & 0x3fff) as u32,
        )),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Fenced-code highlighting
// ---------------------------------------------------------------------------

/// A token class the stylesheet colours. Anything that is none of these is
/// emitted unwrapped, which keeps the payload small: most of a code block is
/// identifiers and punctuation, not tokens.
#[derive(Clone, Copy)]
enum Class {
    Comment,
    Str,
    Number,
    Keyword,
    Tag,
    Attr,
}

impl Class {
    /// Short names on purpose: one span is emitted per token, so a document full
    /// of code repeats the class name hundreds of times.
    fn name(self) -> &'static str {
        match self {
            Class::Comment => "tok-c",
            Class::Str => "tok-s",
            Class::Number => "tok-n",
            Class::Keyword => "tok-k",
            Class::Tag => "tok-t",
            Class::Attr => "tok-a",
        }
    }
}

/// Everything the scanner needs to know about one language, so adding a language
/// is a table entry rather than new code.
struct Language {
    line_comments: &'static [&'static str],
    block_comment: Option<(&'static str, &'static str)>,
    /// Longest first, so `"""` is matched before `"`.
    quotes: &'static [&'static str],
    keywords: &'static [&'static str],
    /// SQL-style: keywords match whatever their case.
    fold_case: bool,
    /// Markup (HTML/XML) is scanned by its own state machine.
    markup: bool,
}

const BLOCK_C: Option<(&str, &str)> = Some(("/*", "*/"));

/// Shared by the C family and the languages whose keywords mostly overlap it
/// (Rust, TypeScript, Go, Java). A word that is a keyword in a sibling language
/// can be coloured here; one table per language would be far more code than the
/// difference is worth.
const KW_C: &[&str] = &[
    "abstract", "and", "as", "async", "auto", "await", "bool", "break", "byte", "case",
    "catch", "chan", "char", "class", "const", "continue", "crate", "defer", "def",
    "default", "delegate", "delete", "do", "double", "dyn", "elif", "else", "enum",
    "except", "export", "extends", "extern", "false", "final", "finally", "float", "fn",
    "for", "foreach", "from", "func", "function", "go", "goto", "if", "impl",
    "implements", "import", "in", "inline", "instanceof", "int", "interface", "internal",
    "is", "let", "long", "loop", "match", "mod", "module", "move", "mut", "namespace",
    "new", "nil", "null", "nullptr", "of", "operator", "or", "override", "package",
    "private", "protected", "protocol", "pub", "public", "range", "ref", "register",
    "return", "select", "self", "short", "signed", "sizeof", "static", "string",
    "struct", "super", "switch", "this", "throw", "throws", "trait", "true", "try",
    "type", "typedef", "typeof", "uint", "ulong", "union", "unsigned", "unsafe",
    "ushort", "use", "using", "var", "virtual", "void", "volatile", "where", "while",
    "with", "yield",
];

const KW_PY: &[&str] = &[
    "and", "as", "assert", "async", "await", "break", "class", "continue", "def", "del",
    "elif", "else", "except", "False", "finally", "for", "from", "global", "if", "import",
    "in", "is", "lambda", "None", "nonlocal", "not", "or", "pass", "raise", "return",
    "True", "try", "while", "with", "yield",
];

/// Shell keywords plus the commands that show up in almost every snippet. A
/// command list is a heuristic, but colouring the verb is most of what makes a
/// shell block scannable.
const KW_SHELL: &[&str] = &[
    "case", "do", "done", "elif", "else", "esac", "fi", "for", "function", "if", "in",
    "local", "return", "select", "then", "until", "while", "export", "source", "set",
    "unset", "read", "exit", "echo", "cd", "ls", "mkdir", "rm", "cp", "mv", "cat",
    "grep", "sed", "awk", "curl", "git", "npm", "npx", "node", "cargo", "make", "sudo",
    "apt", "brew", "python", "pip", "docker", "ssh", "tar", "chmod", "kill", "find",
    "sort", "uniq", "head", "tail", "wc", "tee", "which", "env", "date", "printf",
];

const KW_SQL: &[&str] = &[
    "add", "all", "alter", "and", "as", "asc", "begin", "between", "by", "case", "column",
    "commit", "constraint", "create", "default", "delete", "desc", "distinct", "drop",
    "else", "end", "exists", "foreign", "from", "full", "group", "having", "if", "in",
    "index", "inner", "insert", "into", "is", "join", "key", "left", "like", "limit",
    "not", "null", "on", "or", "order", "outer", "primary", "references", "right",
    "rollback", "select", "set", "table", "then", "union", "unique", "update", "values",
    "when", "where",
];

/// PowerShell, which a Windows-first project's docs reach for often. Backtick is
/// an escape character there rather than a quote, so it is not a delimiter here.
const KW_PS: &[&str] = &[
    "begin", "break", "catch", "class", "continue", "data", "do", "dynamicparam", "else",
    "elseif", "end", "enum", "exit", "filter", "finally", "for", "foreach", "from",
    "function", "hidden", "if", "in", "param", "process", "return", "static", "switch",
    "throw", "trap", "try", "until", "using", "while", "workflow", "true", "false", "null",
];

/// JSON, YAML, TOML and friends: no keywords to speak of, just their literals.
const KW_DATA: &[&str] = &["true", "false", "null", "yes", "no", "on", "off", "none"];

const KW_CSS: &[&str] = &["and", "important", "from", "to", "media", "supports", "keyframes", "not", "only"];

/// The profile for a fence's language tag, or None for a language we do not
/// know — those blocks are left exactly as the renderer emitted them.
fn language_for(info: &str) -> Option<Language> {
    let name = info.trim().to_ascii_lowercase();
    let language = match name.as_str() {
        "rust" | "rs" => Language {
            line_comments: &["//"],
            block_comment: BLOCK_C,
            quotes: &["\""],
            keywords: KW_C,
            fold_case: false,
            markup: false,
        },
        "js" | "javascript" | "jsx" | "mjs" | "cjs" | "ts" | "typescript" | "tsx" | "node" => {
            Language {
                line_comments: &["//"],
                block_comment: BLOCK_C,
                quotes: &["\"", "'", "`"],
                keywords: KW_C,
                fold_case: false,
                markup: false,
            }
        }
        "json" | "json5" | "jsonc" => Language {
            line_comments: &["//"],
            block_comment: BLOCK_C,
            quotes: &["\""],
            keywords: KW_DATA,
            fold_case: false,
            markup: false,
        },
        "py" | "python" | "python3" => Language {
            line_comments: &["#"],
            block_comment: None,
            quotes: &["\"\"\"", "'''", "\"", "'"],
            keywords: KW_PY,
            fold_case: false,
            markup: false,
        },
        "sh" | "shell" | "bash" | "zsh" | "console" => Language {
            line_comments: &["#"],
            block_comment: None,
            quotes: &["\"", "'", "`"],
            keywords: KW_SHELL,
            fold_case: false,
            markup: false,
        },
        "powershell" | "ps1" | "pwsh" => Language {
            line_comments: &["#"],
            block_comment: Some(("<#", "#>")),
            quotes: &["\"", "'"],
            keywords: KW_PS,
            fold_case: false,
            markup: false,
        },
        "sql" => Language {
            line_comments: &["--"],
            block_comment: BLOCK_C,
            quotes: &["'", "\""],
            keywords: KW_SQL,
            fold_case: true,
            markup: false,
        },
        "yaml" | "yml" | "toml" | "ini" => Language {
            line_comments: &["#"],
            block_comment: None,
            quotes: &["\"", "'"],
            keywords: KW_DATA,
            fold_case: false,
            markup: false,
        },
        "css" | "scss" | "less" => Language {
            line_comments: &[],
            block_comment: BLOCK_C,
            quotes: &["\"", "'"],
            keywords: KW_CSS,
            fold_case: false,
            markup: false,
        },
        "html" | "xml" | "xhtml" | "svg" | "vue" => Language {
            line_comments: &[],
            block_comment: Some(("<!--", "-->")),
            quotes: &["\"", "'"],
            keywords: &[],
            fold_case: false,
            markup: true,
        },
        "c" | "h" | "cpp" | "c++" | "cxx" | "hpp" | "java" | "cs" | "csharp" | "go" | "php"
        | "swift" | "kotlin" | "dart" | "scala" | "m" => Language {
            line_comments: &["//"],
            block_comment: BLOCK_C,
            quotes: &["\"", "'"],
            keywords: KW_C,
            fold_case: false,
            markup: false,
        },
        _ => return None,
    };
    Some(language)
}

/// Wrap a code block's tokens in spans. Anything that is not a token is copied
/// through untouched, so the payload only grows where there is something to
/// colour.
fn highlight(code: &str, language: &Language) -> String {
    let mut out = String::with_capacity(code.len() + code.len() / 8);
    let mut plain_from = 0;
    let mut i = 0;
    while i < code.len() {
        match code_token(code, i, language) {
            Some((end, Some(class))) => {
                escape_into(&mut out, &code[plain_from..i]);
                span_into(&mut out, class, &code[i..end]);
                i = end;
                plain_from = end;
            }
            // An identifier that is not a keyword: consumed whole, so that no
            // keyword can match inside a longer name.
            Some((end, None)) => i = end,
            None => i += char_len(code, i),
        }
    }
    escape_into(&mut out, &code[plain_from..]);
    out
}

/// The token starting at `i`, or None when `i` begins ordinary text. The second
/// element of the pair is the span class, or None for "consume this, wrap
/// nothing" — used for identifiers that are not keywords.
fn code_token(code: &str, i: usize, language: &Language) -> Option<(usize, Option<Class>)> {
    let rest = &code[i..];

    for prefix in language.line_comments {
        if rest.starts_with(prefix) {
            let end = rest.find('\n').map(|n| i + n).unwrap_or(code.len());
            return Some((end, Some(Class::Comment)));
        }
    }
    if let Some((open, close)) = language.block_comment {
        if rest.starts_with(open) {
            let end = rest[open.len()..]
                .find(close)
                .map(|n| i + open.len() + n + close.len())
                .unwrap_or(code.len());
            return Some((end, Some(Class::Comment)));
        }
    }
    for quote in language.quotes {
        if rest.starts_with(quote) {
            return Some((string_end(code, i, quote), Some(Class::Str)));
        }
    }

    let first = rest.chars().next()?;
    if first.is_ascii_digit() {
        return Some((number_end(code, i), Some(Class::Number)));
    }
    if first.is_alphabetic() || first == '_' || first == '$' {
        let end = ident_end(code, i);
        let class = if is_keyword(&code[i..end], language) {
            Some(Class::Keyword)
        } else {
            None
        };
        return Some((end, class));
    }
    None
}

/// End of the string opened at `i`. A single-quoted string stops at the newline,
/// so an apostrophe in prose ("don't") cannot swallow the rest of the block;
/// triple-quoted ones are allowed to span lines.
fn string_end(code: &str, i: usize, quote: &str) -> usize {
    let bytes = code.as_bytes();
    let multiline = quote.len() > 1;
    let mut j = i + quote.len();
    while j < code.len() {
        if bytes[j] == b'\\' {
            j += 1;
            if j < code.len() {
                j += char_len(code, j);
            }
            continue;
        }
        if code[j..].starts_with(quote) {
            return j + quote.len();
        }
        if !multiline && bytes[j] == b'\n' {
            return j;
        }
        j += char_len(code, j);
    }
    code.len()
}

fn number_end(code: &str, i: usize) -> usize {
    let bytes = code.as_bytes();
    let mut j = i;
    while j < code.len()
        && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'.' || bytes[j] == b'_')
    {
        j += 1;
    }
    j
}

fn ident_end(code: &str, i: usize) -> usize {
    let bytes = code.as_bytes();
    let mut j = i;
    while j < code.len()
        && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_' || bytes[j] == b'$')
    {
        j += 1;
    }
    j
}

/// Length of the character at `i`, so the scanner never splits a UTF-8 sequence.
fn char_len(code: &str, i: usize) -> usize {
    code[i..].chars().next().map(char::len_utf8).unwrap_or(1)
}

fn is_keyword(word: &str, language: &Language) -> bool {
    if language.fold_case {
        language.keywords.iter().any(|kw| kw.eq_ignore_ascii_case(word))
    } else {
        language.keywords.contains(&word)
    }
}

fn escape_into(out: &mut String, text: &str) {
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(ch),
        }
    }
}

fn span_into(out: &mut String, class: Class, text: &str) {
    out.push_str("<span class=\"");
    out.push_str(class.name());
    out.push_str("\">");
    escape_into(out, text);
    out.push_str("</span>");
}

/// HTML/XML: comments, element names, attribute names and quoted values. Text
/// between tags is left alone, which is why this needs its own state machine
/// rather than the token scanner.
fn highlight_markup(code: &str) -> String {
    let mut out = String::with_capacity(code.len() + 32);
    let mut i = 0;
    let mut in_tag = false;
    let mut seen_name = false;

    while i < code.len() {
        let rest = &code[i..];

        if rest.starts_with("<!--") {
            let end = rest[4..]
                .find("-->")
                .map(|n| i + 4 + n + 3)
                .unwrap_or(code.len());
            span_into(&mut out, Class::Comment, &code[i..end]);
            i = end;
            continue;
        }

        let ch = match rest.chars().next() {
            Some(ch) => ch,
            None => break,
        };
        if ch == '<' {
            in_tag = true;
            seen_name = false;
        } else if ch == '>' {
            in_tag = false;
        } else if in_tag && (ch == '"' || ch == '\'') {
            let end = string_end(code, i, if ch == '"' { "\"" } else { "'" });
            span_into(&mut out, Class::Str, &code[i..end]);
            i = end;
            continue;
        } else if in_tag && (ch.is_alphabetic() || ch == '_' || ch == ':') {
            // The first name inside a tag is the element; the rest are attributes.
            let end = ident_end(code, i);
            let class = if seen_name { Class::Attr } else { Class::Tag };
            seen_name = true;
            span_into(&mut out, class, &code[i..end]);
            i = end;
            continue;
        }

        escape_into(&mut out, &code[i..i + ch.len_utf8()]);
        i += ch.len_utf8();
    }
    out
}

/// Give fenced code blocks their token spans. Unknown languages are left exactly
/// as the renderer emitted them. Runs before sanitising, so the allow-list still
/// has the final say over the spans this produces.
fn highlight_code_blocks(html: String) -> String {
    let mut out: Option<String> = None;
    let mut copied = 0;
    let mut search = 0;

    while let Some(offset) = html[search..].find("<pre><code") {
        // `<pre>` is only an opening; the attributes live on the `<code>` inside.
        let code_at = search + offset + 5;
        let open_end = match tag_end(&html, code_at) {
            Some(end) => end,
            None => break,
        };
        search = open_end + 1;

        let info = attribute_value(&html[code_at..=open_end], "class").unwrap_or_default();
        let language = match language_for(info.strip_prefix("language-").unwrap_or("")) {
            Some(language) => language,
            None => continue,
        };

        let body_start = open_end + 1;
        let close = match html[body_start..].find("</code></pre>") {
            Some(n) => body_start + n,
            None => continue,
        };
        // The renderer escaped the code text; tokenise the real thing and escape
        // each piece on the way back out.
        let source = unescape_entities(&html[body_start..close]);
        let lit = if language.markup {
            highlight_markup(&source)
        } else {
            highlight(&source, &language)
        };

        let buffer = out.get_or_insert_with(|| String::with_capacity(html.len() + 256));
        buffer.push_str(&html[copied..body_start]);
        buffer.push_str(&lit);
        copied = close;
    }

    match out {
        Some(mut buffer) => {
            buffer.push_str(&html[copied..]);
            buffer
        }
        None => html,
    }
}

// ---------------------------------------------------------------------------
// Loading
// ---------------------------------------------------------------------------

fn load_document(path: &Path) -> Document {
    match read_source(path) {
        Ok((source, truncated)) => {
            let metadata = std::fs::metadata(path).ok();
            let dir = parent_dir(path);
            let html = render_markdown_in(&source, Some(Path::new(&dir)));
            Document {
                path: path.display().to_string(),
                name: file_name(path),
                dir,
                html,
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
/// The document icon, carried inside the binary so a portable copy can install the
/// file association without a second file to ship. Generated by
/// `scripts/make-icons.ps1`; the app icon lives in that same file's `icon.ico`.
const FILE_ICON: &[u8] = include_bytes!("../icons/markdown.ico");

/// Write the document icon beside the theme, recent files and window geometry, and
/// return the registry value that points Explorer at it. `None` means it could not
/// be written, in which case callers keep the executable's own icon.
fn install_file_icon() -> Option<String> {
    // %LOCALAPPDATA%\mdviewer - the bundle identifier, and the folder the uninstall
    // hook already deletes.
    let base = std::env::var_os("LOCALAPPDATA")?;
    let path = PathBuf::from(base).join("mdviewer").join("markdown.ico");
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).ok()?;
    }
    // Rewrite only when the length differs, so this is a no-op on every launch after
    // the first and after an upgrade.
    let existing = std::fs::metadata(&path).map(|meta| meta.len()).unwrap_or(0);
    if existing != FILE_ICON.len() as u64 {
        std::fs::write(&path, FILE_ICON).ok()?;
    }
    Some(format!("\"{}\",0", path.display()))
}

/// Windows draws a document's icon from its ProgId, and the installer points that at
/// the executable - so a machine that has been installed but never launched shows the
/// app tile for every `.md`. Once the ProgId is ours, move it to the document icon.
/// Quiet by design: one value, and only when the key already exists.
fn ensure_file_icon() {
    use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE};
    use winreg::RegKey;

    let Some(value) = install_file_icon() else { return };
    let path = format!(r"Software\Classes\{PROG_ID}\DefaultIcon");
    let Ok(key) = RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(&path, KEY_READ | KEY_WRITE)
    else {
        return;
    };
    let current: String = key.get_value("").unwrap_or_default();
    if current != value {
        let _ = key.set_value("", &value);
        // Explorer caches file icons, so tell it the value moved.
        shell_default::notify_shell();
    }
}

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
    extensions: Vec<&'static str>,
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

        // The file icon is the document one rather than the executable's, since
        // Explorer draws it for every .md. Falls back to the executable's own icon if
        // it cannot be written.
        let file_icon = install_file_icon().unwrap_or_else(|| icon.clone());
        let (progid_icon, _) = root
            .create_subkey(format!(r"Software\Classes\{PROG_ID}\DefaultIcon"))
            .map_err(io)?;
        progid_icon.set_value("", &file_icon).map_err(io)?;

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
            extensions: CLAIM_EXTENSIONS.to_vec(),
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

/// The running version, for the About section of the settings panel, so the
/// number shown can never drift from the binary.
#[tauri::command]
fn app_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Whether the window's size and position should be carried across launches. The
/// frontend owns the checkbox; the flag lives next to the geometry so `setup()`
/// can honour it before the webview exists.
#[tauri::command]
fn set_remember_geometry(app: AppHandle, on: bool) {
    set_remembering(&app, on);
}

/// The subset of `paths` that still exists as a file. The empty state calls this
/// on start-up so recent entries whose file was moved or deleted quietly
/// disappear, instead of failing when someone clicks them.
#[tauri::command]
fn filter_existing(paths: Vec<String>) -> Vec<String> {
    paths
        .into_iter()
        .filter(|path| Path::new(path).is_file())
        .collect()
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

/// What the command line asked for. Flags win over file arguments; `--` ends
/// flag parsing, so `mdviewer -- --version` still tries to open a file called
/// `--version`.
#[derive(Debug, PartialEq)]
enum StartupAction {
    /// Launch the window, optionally with a document.
    Gui(Option<PathBuf>),
    /// `--version`
    Version,
    /// `--help`
    Help,
    /// `--register-default`: write the per-user association keys and exit.
    RegisterDefault,
}

fn classify_args(args: &[String]) -> StartupAction {
    for raw in args.iter().skip(1) {
        let arg = raw.trim().trim_matches('"');
        if arg == "--" {
            break;
        }
        match arg.to_ascii_lowercase().as_str() {
            "--version" | "-v" => return StartupAction::Version,
            "--help" | "-h" => return StartupAction::Help,
            "--register-default" | "--register" => return StartupAction::RegisterDefault,
            _ => {}
        }
    }
    StartupAction::Gui(parse_cli_path(args))
}

const USAGE: &str = "\
MD Viewer {version} - a fast, minimal Markdown viewer

Usage:
  mdviewer [FILE.md]             open a document
  mdviewer --register-default    register this executable as a .md handler
  mdviewer --version             print the version and exit
  mdviewer --help                print this text and exit

--register-default writes the per-user registry keys and refreshes the shell, so
it is what a scripted install (a .wsb logon command, for instance) should call.
Windows still asks the user once before .md opens with MD Viewer by default.
";

/// Release builds have no console, so a flag typed in a terminal would be silent.
/// Attach to the parent console when there is one, and write through the handle
/// directly: the standard streams were bound before the attach happened.
#[cfg(windows)]
fn print_to_console(text: &str) {
    use std::io::Write;
    use std::os::windows::io::FromRawHandle;
    use windows::Win32::System::Console::{
        AttachConsole, GetStdHandle, ATTACH_PARENT_PROCESS, STD_OUTPUT_HANDLE,
    };

    unsafe {
        let _ = AttachConsole(ATTACH_PARENT_PROCESS);
        if let Ok(handle) = GetStdHandle(STD_OUTPUT_HANDLE) {
            if !handle.is_invalid() {
                let mut out = std::fs::File::from_raw_handle(handle.0);
                let _ = out.write_all(text.as_bytes());
                let _ = out.flush();
                std::mem::forget(out); // the handle is shared; never close it here
            }
        }
    }
}

#[cfg(not(windows))]
fn print_to_console(text: &str) {
    use std::io::Write;
    let _ = std::io::stdout().write_all(text.as_bytes());
}

// ---------------------------------------------------------------------------
// Window geometry
// ---------------------------------------------------------------------------

/// Where the window sat when it was last closed, in logical pixels so a DPI
/// change or a different monitor does not resize it.
#[derive(Serialize, Deserialize)]
struct Geometry {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

fn geometry_path(app: &AppHandle) -> Option<PathBuf> {
    app.path()
        .app_local_data_dir()
        .ok()
        .map(|dir| dir.join("window.json"))
}

fn read_geometry(app: &AppHandle) -> Option<Geometry> {
    let path = geometry_path(app)?;
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

fn write_geometry(app: &AppHandle, geometry: &Geometry) {
    let Some(path) = geometry_path(app) else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(text) = serde_json::to_string(geometry) {
        let _ = std::fs::write(path, text);
    }
}

fn remember_path(app: &AppHandle) -> Option<PathBuf> {
    app.path()
        .app_local_data_dir()
        .ok()
        .map(|dir| dir.join("window-remember.txt"))
}

/// Absent means on, so the behaviour an existing install already has is kept.
fn remembering(app: &AppHandle) -> bool {
    remember_path(app)
        .and_then(|path| std::fs::read_to_string(path).ok())
        .map(|text| text.trim() != "off")
        .unwrap_or(true)
}

fn set_remembering(app: &AppHandle, on: bool) {
    let Some(path) = remember_path(app) else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(path, if on { "on" } else { "off" });
}

/// Restore the last size and position. The position is only applied when it still
/// lands on a monitor: unplugging a display must not strand the window off-screen.
fn restore_geometry(window: &tauri::WebviewWindow) {
    let app = window.app_handle().clone();
    if !remembering(&app) {
        return;
    }
    let Some(saved) = read_geometry(&app) else { return };

    let _ = window.set_size(tauri::Size::Logical(tauri::LogicalSize::new(
        saved.width as f64,
        saved.height as f64,
    )));

    let on_screen = window.available_monitors().map(|monitors| {
        monitors.iter().any(|monitor| {
            let scale = monitor.scale_factor();
            let origin = monitor.position().to_logical::<f64>(scale);
            let size = monitor.size().to_logical::<f64>(scale);
            let x = saved.x as f64;
            let y = saved.y as f64;
            x >= origin.x && y >= origin.y
                && x < origin.x + size.width
                && y < origin.y + size.height
        })
    });
    if on_screen.unwrap_or(false) {
        let _ = window.set_position(tauri::Position::Logical(tauri::LogicalPosition::new(
            saved.x as f64,
            saved.y as f64,
        )));
    }
}

/// Called when the window is closing, which is the last moment its geometry is
/// still readable.
fn save_geometry(window: &tauri::Window) {
    if !remembering(&window.app_handle().clone()) {
        return;
    }
    // inner_size, because set_size restores the client area; saving the outer size
    // would make the window grow by its frame on every launch.
    let (Ok(position), Ok(size)) = (window.outer_position(), window.inner_size()) else {
        return;
    };
    let scale = window.scale_factor().unwrap_or(1.0);
    let origin = position.to_logical::<f64>(scale);
    let extent = size.to_logical::<f64>(scale);
    write_geometry(
        &window.app_handle().clone(),
        &Geometry {
            x: origin.x.round() as i32,
            y: origin.y.round() as i32,
            width: extent.width.round() as u32,
            height: extent.height.round() as u32,
        },
    );
}

fn run(startup_path: Option<PathBuf>) {
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
            default_status,
            filter_existing,
            app_version,
            set_remember_geometry
        ])
        .setup(move |app| {
            let handle = app.handle().clone();

            // A fresh install leaves the file icon pointing at the executable; move it
            // to the document icon once, quietly. No-op when we are not registered.
            ensure_file_icon();

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

            // Put the window back where it was last closed, before it is shown.
            if let Some(window) = handle.get_webview_window("main") {
                restore_geometry(&window);
            }

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
        .on_window_event(|window, event| {
            // The last moment the geometry is readable; the window is already gone
            // by the time the run loop exits.
            if matches!(event, tauri::WindowEvent::CloseRequested { .. }) {
                save_geometry(window);
            }
        })
        .build(tauri::generate_context!())
        .expect("failed to build MD Viewer")
        .run(|_app, _event| {});
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    match classify_args(&args) {
        StartupAction::Version => {
            print_to_console(&format!("mdviewer {}\n", env!("CARGO_PKG_VERSION")));
        }
        StartupAction::Help => {
            print_to_console(&USAGE.replace("{version}", env!("CARGO_PKG_VERSION")));
        }
        StartupAction::RegisterDefault => {
            match current_program().and_then(|exe| shell_default::register(&exe).map(|_| exe)) {
                Ok(exe) => {
                    shell_default::notify_shell();
                    print_to_console(&format!(
                        "Registered {} as a handler for .md and friends.\n\
                         Windows still needs one confirmation: right-click a .md file, \
                         Open with, choose MD Viewer, then Always.\n",
                        exe.display()
                    ));
                }
                Err(error) => {
                    print_to_console(&format!("Could not register: {error}\n"));
                    std::process::exit(1);
                }
            }
        }
        StartupAction::Gui(path) => run(path),
    }
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

    fn args_of(list: &[&str]) -> Vec<String> {
        list.iter().map(|item| item.to_string()).collect()
    }

    #[test]
    fn cli_flags_are_recognised() {
        assert_eq!(classify_args(&args_of(&["mdviewer.exe", "--version"])), StartupAction::Version);
        assert_eq!(classify_args(&args_of(&["mdviewer.exe", "-V"])), StartupAction::Version);
        assert_eq!(classify_args(&args_of(&["mdviewer.exe", "--help"])), StartupAction::Help);
        assert_eq!(classify_args(&args_of(&["mdviewer.exe", "-h"])), StartupAction::Help);
        assert_eq!(
            classify_args(&args_of(&["mdviewer.exe", "--register-default"])),
            StartupAction::RegisterDefault
        );
        assert_eq!(classify_args(&args_of(&["mdviewer.exe"])), StartupAction::Gui(None));
    }

    #[test]
    fn cli_flags_do_not_shadow_a_document() {
        let file = temp_file("flags-doc.md", b"# hi");
        let path = file.display().to_string();
        assert_eq!(
            classify_args(&args_of(&["mdviewer.exe", &path])),
            StartupAction::Gui(Some(file.clone()))
        );
        let _ = std::fs::remove_file(file);
    }

    #[test]
    fn double_dash_ends_flag_parsing() {
        // After `--`, a flag-looking argument is a path, not a flag. No such file
        // exists here, so the result is "GUI, no document".
        assert_eq!(
            classify_args(&args_of(&["mdviewer.exe", "--", "--version"])),
            StartupAction::Gui(None)
        );
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
        let html = render_markdown_in(source, None);

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
        let html = render_markdown_in(
            "<script>steal()</script>\n\n<img src=x onerror=\"steal()\">\n\n[click](javascript:steal())\n\n<div style=\"position:fixed\" onclick=\"steal()\">x</div>",
            None,
        );

        assert!(!html.contains("<script"), "script tag: {html}");
        assert!(!html.contains("onerror"), "event handler: {html}");
        assert!(!html.contains("javascript:"), "javascript url: {html}");
        assert!(!html.contains("onclick"), "inline handler: {html}");
        assert!(!html.contains("position:fixed"), "unsafe style: {html}");
    }

    /// The webview needs width/height to reserve an image's box before it loads.
    /// Values come from the file header (no decoding, no dependency), the author's
    /// own attributes win, and non-local sources are left alone.
    #[test]
    fn local_image_dimensions_come_from_the_file_header() {
        let dir = std::env::temp_dir().join("mdviewer-image-dimensions-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        // 640x480 PNG
        let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
        png.extend_from_slice(&13u32.to_be_bytes());
        png.extend_from_slice(b"IHDR");
        png.extend_from_slice(&640u32.to_be_bytes());
        png.extend_from_slice(&480u32.to_be_bytes());
        std::fs::write(dir.join("shot.png"), &png).unwrap();

        // 320x200 JPEG (SOI, then a SOF0 frame header)
        let mut jpeg = vec![0xff, 0xd8, 0xff, 0xc0, 0x00, 0x11, 0x08];
        jpeg.extend_from_slice(&200u16.to_be_bytes());
        jpeg.extend_from_slice(&320u16.to_be_bytes());
        jpeg.extend_from_slice(&[0x03, 0x01, 0x11, 0x00, 0x02, 0x11, 0x00, 0x03, 0x11, 0x00]);
        std::fs::write(dir.join("photo.jpg"), &jpeg).unwrap();

        // 64x32 GIF
        let mut gif = b"GIF89a".to_vec();
        gif.extend_from_slice(&64u16.to_le_bytes());
        gif.extend_from_slice(&32u16.to_le_bytes());
        std::fs::write(dir.join("anim.gif"), &gif).unwrap();

        // 100x50 WebP (VP8X)
        let mut webp = b"RIFF".to_vec();
        webp.extend_from_slice(&0u32.to_le_bytes());
        webp.extend_from_slice(b"WEBP");
        webp.extend_from_slice(b"VP8X");
        webp.extend_from_slice(&10u32.to_le_bytes());
        webp.push(0);
        webp.extend_from_slice(&[0, 0, 0]);
        webp.extend_from_slice(&[99, 0, 0]);
        webp.extend_from_slice(&[49, 0, 0]);
        std::fs::write(dir.join("pic.webp"), &webp).unwrap();

        // A file whose name needs unescaping in the src.
        std::fs::write(dir.join("my shot & more.png"), &png).unwrap();

        let source = concat!(
            "![shot](shot.png)\n\n",
            "![photo](photo.jpg)\n\n",
            "![anim](anim.gif)\n\n",
            "![pic](pic.webp)\n\n",
            "![spaced](my%20shot%20&amp;%20more.png)\n\n",
            "![remote](https://example.com/x.png)\n\n",
            "![missing](nope.png)\n\n",
            "<img src=\"shot.png\" width=\"10\" height=\"10\">\n"
        );
        let html = render_markdown_in(source, Some(&dir));

        assert!(html.contains("alt=\"shot\" width=\"640\" height=\"480\""), "png: {html}");
        assert!(html.contains("alt=\"photo\" width=\"320\" height=\"200\""), "jpeg: {html}");
        assert!(html.contains("alt=\"anim\" width=\"64\" height=\"32\""), "gif: {html}");
        assert!(html.contains("alt=\"pic\" width=\"100\" height=\"50\""), "webp: {html}");
        assert!(
            html.contains("alt=\"spaced\" width=\"640\" height=\"480\""),
            "escaped path: {html}"
        );
        // Five decodable local images, plus the author's own width/height pair.
        assert_eq!(html.matches("width=\"").count(), 6, "width attrs in:\n{html}");
        assert_eq!(html.matches("height=\"").count(), 6, "height attrs in:\n{html}");
        assert!(html.contains("width=\"10\" height=\"10\""), "author values kept: {html}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A render without a document folder (used by most tests and by any caller
    /// with no path) must not try to touch the filesystem.
    #[test]
    fn image_dimensions_are_skipped_without_a_folder() {
        let html = render_markdown_in("![a](shot.png)\n", None);
        assert_eq!(html.matches("width=\"").count(), 0, "{html}");
    }

    /// The document icon has to survive being embedded in the binary, and the value
    /// handed to Windows has to be the `"path",0` shape it expects - otherwise every
    /// `.md` silently falls back to the app tile.
    #[test]
    fn the_file_icon_is_embedded_and_installable() {
        assert!(FILE_ICON.len() > 1000, "the document icon looks empty");
        let value = install_file_icon().expect("write the document icon");
        assert!(value.starts_with('"') && value.ends_with("\",0"), "value shape: {value}");

        let file = value.trim_start_matches('"').trim_end_matches("\",0");
        assert!(file.ends_with("markdown.ico"), "icon path: {file}");
        let written = std::fs::read(file).expect("the icon is on disk");
        assert_eq!(written, FILE_ICON, "the icon must round-trip unchanged");
    }

    /// The geometry file outlives a build, so its shape must round-trip: a field
    /// rename would silently throw away everyone's saved window.
    #[test]
    fn window_geometry_round_trips_through_json() {
        let saved = Geometry { x: -1280, y: 40, width: 900, height: 780 };
        let text = serde_json::to_string(&saved).unwrap();
        let parsed: Geometry = serde_json::from_str(&text).unwrap();
        assert_eq!(parsed.x, -1280, "a monitor to the left is a negative origin");
        assert_eq!(parsed.y, 40);
        assert_eq!(parsed.width, 900);
        assert_eq!(parsed.height, 780);
    }

    /// Fenced code gets token spans so the stylesheet can colour it. Only the
    /// tokens are wrapped, the language class survives for the code toolbar, and
    /// a language we do not know is left exactly as the renderer wrote it.
    #[test]
    fn fenced_code_is_highlighted() {
        let html = render_markdown_in("```rust\n// note\nfn main() { let x = \"hi\"; }\n```\n", None);
        assert!(html.contains("<span class=\"tok-c\">// note</span>"), "{html}");
        assert!(html.contains("<span class=\"tok-k\">fn</span>"), "{html}");
        assert!(html.contains("<span class=\"tok-k\">let</span>"), "{html}");
        // A double quote in text needs no escaping, so the sanitiser leaves it raw.
        assert!(html.contains("<span class=\"tok-s\">\"hi\"</span>"), "{html}");
        assert!(html.contains("class=\"language-rust\""), "{html}");
        // Identifiers and punctuation are not wrapped.
        assert!(html.contains(" main()"), "{html}");

        let plain = render_markdown_in("```\nno language here\n```\n", None);
        assert!(!plain.contains("tok-"), "{plain}");
        let unknown = render_markdown_in("```cobol\nDISPLAY 'HI'.\n```\n", None);
        assert!(!unknown.contains("tok-"), "{unknown}");
    }

    /// The code text arrives escaped and has to go back out escaped, or the
    /// highlighted block would inject markup.
    #[test]
    fn highlighting_keeps_code_escaped() {
        let html = render_markdown_in("```html\n<a href=\"x\">&amp;</a>\n```\n", None);
        assert!(html.contains("<span class=\"tok-t\">a</span>"), "{html}");
        assert!(html.contains("<span class=\"tok-a\">href</span>"), "{html}");
        // The tag's own angle brackets are code text, so they leave escaped.
        assert!(html.contains("&lt;") && html.contains("&gt;"), "{html}");
        assert!(html.contains("&amp;amp;"), "the ampersand must stay escaped: {html}");

        let injected = render_markdown_in("```js\nlet s = \"<img src=x onerror=alert(1)>\";\n```\n", None);
        assert!(!injected.contains("<img"), "markup from inside code leaked: {injected}");
    }

    #[test]
    fn sql_keywords_ignore_case() {
        let html = render_markdown_in("```sql\nSELECT id FROM t WHERE x = 1\n```\n", None);
        assert!(html.contains("<span class=\"tok-k\">SELECT</span>"), "{html}");
        assert!(html.contains("<span class=\"tok-k\">FROM</span>"), "{html}");
        assert!(html.contains("<span class=\"tok-n\">1</span>"), "{html}");
    }

    #[test]
    fn python_triple_quotes_and_comments() {
        let source = "```python\ndef f():\n    '''doc'''  # trailing\n    return \"x\"\n```\n";
        let html = render_markdown_in(source, None);
        assert!(html.contains("<span class=\"tok-k\">def</span>"), "{html}");
        assert!(html.contains("<span class=\"tok-s\">'''doc'''</span>"), "{html}");
        assert!(html.contains("<span class=\"tok-c\"># trailing</span>"), "{html}");
    }

    /// PowerShell is what a Windows project's own docs reach for, so the fence
    /// tag has to be known (its backtick is an escape, not a quote delimiter).
    #[test]
    fn powershell_fences_are_highlighted() {
        let source = "```powershell\n# note\nfunction Get-Thing { param($name) return \"$name\" }\n```\n";
        let html = render_markdown_in(source, None);
        assert!(html.contains("<span class=\"tok-c\"># note</span>"), "{html}");
        assert!(html.contains("<span class=\"tok-k\">function</span>"), "{html}");
        assert!(html.contains("<span class=\"tok-k\">param</span>"), "{html}");
        assert!(html.contains("<span class=\"tok-k\">return</span>"), "{html}");
    }

    /// Type names are keywords in the C family, and they are most of what makes a
    /// C or Java block look highlighted at all.
    #[test]
    fn c_family_types_are_keywords() {
        let html = render_markdown_in("```c\nint main(void) { char c = 'x'; return 0; }\n```\n", None);
        assert!(html.contains("<span class=\"tok-k\">int</span>"), "{html}");
        assert!(html.contains("<span class=\"tok-k\">char</span>"), "{html}");
        assert!(html.contains("<span class=\"tok-k\">return</span>"), "{html}");
    }

    #[test]
    fn table_alignment_is_emitted_as_classes() {
        let html = render_markdown_in("| a | b |\n| :- | -: |\n| 1 | 2 |\n", None);
        // Inline styles are blocked by the app's CSP, so alignment is a class.
        assert!(html.contains("class=\"align-left\""), "left alignment: {html}");
        assert!(html.contains("class=\"align-right\""), "right alignment: {html}");
        assert!(!html.contains("style="), "no inline styles may survive: {html}");
    }

    #[test]
    fn inline_styles_from_raw_html_are_dropped() {
        let html = render_markdown_in("<div style=\"color:red;text-align:center\">x</div>", None);
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
    fn filter_existing_keeps_only_real_files() {        let real = temp_file("filter-existing.md", b"# hi");
        let missing = std::env::temp_dir().join("mdviewer-definitely-missing-77.md");
        let kept = filter_existing(vec![
            real.display().to_string(),
            missing.display().to_string(),
            String::new(),
        ]);
        assert_eq!(kept, vec![real.display().to_string()]);
        let _ = std::fs::remove_file(real);
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
        let html = render_markdown_in(&source, None);
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
