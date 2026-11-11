/* MD Viewer frontend.
 * Plain ES2020, no bundler, no framework. Talks to Rust over Tauri IPC:
 *   commands : ready, initial_document, open_dialog, load_path, open_link, reload
 *   events   : md://open, md://reload, md://error, md://notice, md://drag
 */
(function () {
  "use strict";

  var T = window.__TAURI__ || {};
  var invoke = (T.core && T.core.invoke) || function () {
    return Promise.reject(new Error("Tauri IPC unavailable"));
  };
  var listen = (T.event && T.event.listen) || function () {
    return Promise.resolve(function () {});
  };
  var convertFileSrc = (T.core && T.core.convertFileSrc) || function (p) {
    // Same shape Tauri uses on Windows: http://asset.localhost/<encodeURIComponent(path)>
    return "http://asset.localhost/" + encodeURIComponent(p);
  };
  // Hands back to the browser between progressive chunks so the top of a long
  // document paints while the rest is still being built. Injectable because
  // requestAnimationFrame does not fire in an occluded tab, which is how the
  // preview harness runs; read late so a test can swap it between renders.
  function scheduleChunk(fn) {
    if (T.scheduler && T.scheduler.next) { T.scheduler.next(fn); return; }
    requestAnimationFrame(fn);
  }

  /* ------------------------------------------------------------------ *
   * Theme — resolved before first paint because this script is in <head>
   * ------------------------------------------------------------------ */

  var THEME_KEY = "mdviewer.theme";
  var SCALE_KEY = "mdviewer.scale";
  var RECENT_KEY = "mdviewer.recent";
  var POSITION_KEY = "mdviewer.positions";
  var themePref = localStorage.getItem(THEME_KEY) || "auto";
  var darkQuery = window.matchMedia("(prefers-color-scheme: dark)");

  function resolvedTheme() {
    if (themePref === "light" || themePref === "dark") return themePref;
    return darkQuery.matches ? "dark" : "light";
  }

  function applyTheme() {
    document.documentElement.setAttribute("data-theme", resolvedTheme());
    document.documentElement.setAttribute("data-theme-pref", themePref);
  }

  function applyScale(scale) {
    var value = Math.min(1.6, Math.max(0.8, scale));
    document.documentElement.style.setProperty("--reading-scale", String(value));
    localStorage.setItem(SCALE_KEY, String(value));
    return value;
  }

  applyTheme();
  var readingScale = applyScale(parseFloat(localStorage.getItem(SCALE_KEY) || "1") || 1);

  darkQuery.addEventListener("change", function () {
    if (themePref === "auto") applyTheme();
  });

  /* ------------------------------------------------------------------ *
   * Small helpers
   * ------------------------------------------------------------------ */

  var $ = function (id) { return document.getElementById(id); };
  var noop = function () {};

  function fmtInt(n) {
    try { return new Intl.NumberFormat().format(n); } catch (_) { return String(n); }
  }

  function fmtStamp(ms) {
    if (!ms) return "";
    try {
      return new Date(ms).toLocaleString(undefined, {
        year: "numeric", month: "short", day: "numeric",
        hour: "2-digit", minute: "2-digit", second: "2-digit"
      });
    } catch (_) { return ""; }
  }

  function slugify(text) {
    var lowered = String(text).trim().toLowerCase();
    // Fast path for the overwhelmingly common ASCII heading: the unicode property
    // escapes below were the bulk of the cost for documents with hundreds of
    // headings.
    if (/^[\x20-\x7e]*$/.test(lowered)) {
      return lowered
        .replace(/[^a-z0-9\s-]/g, "")
        .replace(/\s+/g, "-")
        .replace(/-{2,}/g, "-")
        .replace(/^-|-$/g, "") || "section";
    }
    return lowered
      .replace(/[\u2019'"`]/g, "")
      .replace(/[^\p{L}\p{N}\s-]/gu, "")
      .replace(/\s+/g, "-")
      .replace(/-{2,}/g, "-")
      .replace(/^-|-$/g, "") || "section";
  }

  async function copyText(text) {
    try {
      if (navigator.clipboard && navigator.clipboard.writeText) {
        await navigator.clipboard.writeText(text);
        return true;
      }
    } catch (_) { /* fall through to the legacy path */ }
    try {
      var ta = document.createElement("textarea");
      ta.value = text;
      ta.setAttribute("readonly", "");
      ta.style.position = "fixed";
      ta.style.top = "-1000px";
      ta.style.opacity = "0";
      document.body.appendChild(ta);
      ta.select();
      var ok = document.execCommand("copy");
      ta.remove();
      return ok;
    } catch (_) {
      return false;
    }
  }

  /* ------------------------------------------------------------------ *
   * Boot
   * ------------------------------------------------------------------ */

  function boot() {
    var docEl = $("doc");
    var skipLink = $("skip-link");
    var welcome = $("welcome");
    var errorCard = $("error");
    var welcomeError = $("welcome-error");
    var footer = $("doc-footer");
    var toc = $("toc");
    var tocScrim = $("toc-scrim");
    var tocList = $("toc-list");
    var progress = $("progress");
    var toastEl = $("toast");
    var dropzone = $("dropzone");

    var current = null;      // last rendered document payload
    var headings = [];       // [{ id, level, el }]
    var spinner = null;

    /* ---------- toast ---------- */

    var toastTimer = null;
    function toast(message) {
      if (!message) return;
      toastEl.textContent = message;
      toastEl.hidden = false;
      requestAnimationFrame(function () { toastEl.classList.add("show"); });
      clearTimeout(toastTimer);
      toastTimer = setTimeout(function () {
        toastEl.classList.remove("show");
        setTimeout(function () { toastEl.hidden = true; }, 200);
      }, 2400);
    }

    /* ---------- welcome / recent ---------- */

    function readRecent() {
      try {
        var raw = JSON.parse(localStorage.getItem(RECENT_KEY) || "[]");
        return Array.isArray(raw) ? raw : [];
      } catch (_) { return []; }
    }

    function pushRecent(doc) {
      if (!doc || !doc.path) return;
      var list = readRecent().filter(function (item) { return item.path !== doc.path; });
      list.unshift({ path: doc.path, name: doc.name, dir: doc.dir, at: Date.now() });
      try { localStorage.setItem(RECENT_KEY, JSON.stringify(list.slice(0, 6))); } catch (_) {}
      renderRecent();
    }

    function renderRecent() {
      var wrap = $("recent-wrap");
      var list = $("recent-list");
      var items = readRecent();
      list.textContent = "";
      renderReopen();
      if (!items.length) { wrap.hidden = true; return; }
      items.forEach(function (item) {
        var btn = document.createElement("button");
        btn.type = "button";
        btn.className = "recent-item";
        btn.title = item.path;
        var name = document.createElement("span");
        name.className = "recent-name";
        name.textContent = item.name || item.path;
        var dir = document.createElement("span");
        dir.className = "recent-dir";
        dir.textContent = item.dir || "";
        btn.append(name, dir);
        btn.addEventListener("click", function () { openPath(item.path); });
        list.appendChild(btn);
      });
      wrap.hidden = false;
    }

    /* Session continuity: with no file on the command line, the welcome screen
     * offers the document that was last on screen as the lead action. Nothing is
     * stored — the last document is simply the top of the recent list — so there
     * is no switch to get stuck behind. */
    function renderReopen() {
      var reopen = $("welcome-reopen");
      var open = $("welcome-open");
      var last = readRecent()[0];
      if (!last || !last.path) {
        reopen.hidden = true;
        open.className = "btn-primary";
        return;
      }
      $("reopen-name").textContent = last.name || last.path;
      reopen.title = "Reopen " + last.path;
      reopen.hidden = false;
      // Exactly one primary action on screen: resume leads, opening steps back.
      open.className = "btn-ghost";
    }

    function showWelcome() {
      welcome.hidden = false;
      docEl.hidden = true;
      footer.hidden = true;
      errorCard.hidden = true;
      skipLink.hidden = true;
      setActiveHeadings([]);
      closeToc();
      document.title = "MD Viewer";
      $("doc-title").textContent = "Markdown viewer";
      $("doc-sub").textContent = "";
      $("doc-meta").textContent = "";
      updateProgress();
    }

    function showWelcomeError(message) {
      welcome.hidden = false;
      welcomeError.hidden = false;
      welcomeError.textContent = message;
    }

    function showDocError(message, canRetry) {
      errorCard.hidden = false;
      $("error-text").textContent = message;
      $("error-retry").hidden = !canRetry;
    }

    /* ---------- document rendering ---------- */

    // Rendered HTML above this is built in chunks instead of one innerHTML call.
    // Opening a large document is layout-bound, not parse-bound: a 1.2 MB
    // document spent ~133 ms parsing and ~1.9 s in the browser's layout passes, so
    // the win is painting the first chunk long before the last one is built.
    var PROGRESSIVE_HTML = 1 << 20;      // 1 MiB of HTML (~400 KiB of markdown)
    var PROGRESSIVE_CHUNK = 128 << 10;   // 128 KiB per chunk (measured sweet spot)
    var renderToken = 0;                 // bumped per render; cancels a chunk pump

    // Elements that never need a matching close tag, so they must not change the
    // nesting depth while scanning.
    var VOID_ELEMENTS = {
      area: 1, base: 1, br: 1, col: 1, embed: 1, hr: 1, img: 1, input: 1,
      link: 1, meta: 1, param: 1, source: 1, track: 1, wbr: 1
    };

    // Start of the tag that opens at `lt`, past any ">" that sits inside a quoted
    // attribute value (the sanitizer escapes "<", "&" and '"' but not ">", so a
    // link title can legitimately contain one).
    function tagEnd(html, lt) {
      var i = lt + 1;
      var quote = 0;
      while (i < html.length) {
        var c = html.charAt(i);
        if (quote) { if (c === quote) quote = 0; }
        else if (c === '"' || c === "'") quote = c;
        else if (c === ">") return i;
        i++;
      }
      return -1;
    }

    /* Split rendered HTML into chunks no larger than `maxBytes`, cutting only
     * where the nesting is back at the top level: each chunk is then a whole
     * sequence of block elements, so appending the chunks in order produces the
     * same DOM as inserting the string in one go. A single element bigger than
     * `maxBytes` is left intact (one oversized chunk) rather than torn apart. */
    function splitTopLevelHtml(html, maxBytes) {
      var chunks = [];
      var depth = 0;
      var start = 0;
      var i = 0;
      while (i < html.length) {
        var lt = html.indexOf("<", i);
        if (lt === -1) break;
        if (html.startsWith("<!--", lt)) {
          var close = html.indexOf("-->", lt + 4);
          i = close === -1 ? html.length : close + 3;
          continue;
        }
        var gt = tagEnd(html, lt);
        if (gt === -1) break;
        var tag = html.slice(lt + 1, gt);
        if (tag.charAt(0) === "/") {
          depth--;
        } else if (tag.charAt(tag.length - 1) !== "/") {
          if (!VOID_ELEMENTS[tag.split(/[\s/]/)[0].toLowerCase()]) depth++;
        }
        i = gt + 1;
        if (depth <= 0 && i - start >= maxBytes && i < html.length) {
          chunks.push(html.slice(start, i));
          start = i;
        }
      }
      if (start < html.length) chunks.push(html.slice(start));
      return chunks;
    }

    function render(doc, options) {
      options = options || {};
      if (!doc) { showWelcome(); return; }
      if (doc.error) {
        var message = "Could not open " + (doc.path || "file") + "\n" + doc.error;
        if (current && !options.reset) {
          // Keep the last good render on screen: an editor that saves by
          // deleting and rewriting the file must not blank the article.
          showDocError(message, true);
        } else {
          welcomeError.textContent = message;
          welcomeError.hidden = false;
          showWelcomeState();
        }
        toast(doc.error);
        return;
      }

      var previousScroll = options.preserveScroll ? window.scrollY : 0;
      var sameFile = current && current.path === doc.path;

      current = doc;
      welcome.hidden = true;
      welcomeError.hidden = true;
      errorCard.hidden = true;
      docEl.hidden = false;
      footer.hidden = false;
      skipLink.hidden = false;

      var token = ++renderToken;

      // A large document is built and attached a chunk at a time so the first
      // screen paints long before the last chunk exists; a small one is faster
      // in a single pass.
      if ((doc.html || "").length >= PROGRESSIVE_HTML) {
        renderProgressive(doc, options, previousScroll, sameFile, token);
        return;
      }

      // Mutating the live article is fine: the browser defers style/layout until
      // the task ends, so enhance + outline cost one pass (staging the tree in a
      // detached container and moving it in measured *slower* — an extra tree move
      // for no layout saving: 211 ms vs 131 ms on an 80 KiB document).
      docEl.innerHTML = doc.html || "";

      headings = enhance(docEl);
      headingTops = null;   // offsets changed with the document
      buildToc(headings);
      finishRender(doc, options, previousScroll, sameFile);
    }

    /* Everything that needs the whole document already in the DOM. */
    function finishRender(doc, options, previousScroll, sameFile) {
      updateHeader(doc);
      updateFooter(doc);
      rewriteLocalImages(doc);

      // Local images can reflow the page when they finish loading, which moves
      // every heading after them; drop the cached offsets when that happens.
      docEl.querySelectorAll("img").forEach(function (img) {
        if (img.complete) return;
        img.addEventListener("load", function () { headingTops = null; }, { once: true });
      });

      if (sameFile && options.preserveScroll) {
        // A live reload of the document already on screen: keep reading position.
        window.scrollTo({ top: previousScroll, behavior: "instant" });
      } else if (sameFile) {
        // Explicitly re-opened the same file: start at the top.
        window.scrollTo({ top: 0, behavior: "instant" });
      } else {
        // A different document: resume where it was left. Images may still be
        // loading, so the browser clamps this to the height that exists yet.
        var stored = readPositions()[doc.path];
        var target = typeof stored === "number" ? stored : 0;
        window.scrollTo({ top: target, behavior: "instant" });
        if (target > 200) {
          setTimeout(function () {
            toast("Resumed where you left off — Home jumps to the top");
          }, 500);
        }
      }
      updateProgress();
      updateActiveHeading();
      pushRecent(doc);
      find.refresh();

      if (doc.truncated) {
        setTimeout(function () {
          toast("Large file — rendering the first " + fmtInt(16) + " MB only");
        }, 400);
      }
      if (options.notice) toast(options.notice);
    }

    /* Build a large document in top-level chunks, yielding to the browser between
     * them. `headings` grows as chunks arrive so the outline's scroll-spy stays
     * usable while the rest is still being attached. */
    function renderProgressive(doc, options, previousScroll, sameFile, token) {
      var chunks = splitTopLevelHtml(doc.html || "", PROGRESSIVE_CHUNK);
      var state = { used: Object.create(null), found: [] };
      var stage = document.createElement("div");
      var index = 0;

      headings = state.found;
      docEl.textContent = "";
      tocList.textContent = "";

      function step() {
        if (token !== renderToken) return;   // a newer render has taken over
        stage.innerHTML = chunks[index++];
        var from = state.found.length;
        enhance(stage, state);
        appendToc(state.found, from);
        var frag = document.createDocumentFragment();
        while (stage.firstChild) frag.appendChild(stage.firstChild);
        docEl.appendChild(frag);
        if (index < chunks.length) { scheduleChunk(step); return; }
        headingTops = null;
        if (state.found.length < 2) buildToc(state.found);   // "no headings" note
        finishRender(doc, options, previousScroll, sameFile);
      }
      step();
    }

    function showWelcomeState() {
      welcome.hidden = false;
      docEl.hidden = true;
      footer.hidden = true;
      docEl.textContent = "";
      current = null;
    }

    /* Add ids to headings, wrap tables/pre and give code blocks a toolbar.
     * A progressive render passes a shared `state` so ids stay unique and the
     * heading list accumulates across chunks. */
    function enhance(root, state) {
      var found = state ? state.found : [];
      var used = state ? state.used : Object.create(null);

      root.querySelectorAll("h1, h2, h3, h4, h5, h6").forEach(function (el) {
        var id = el.getAttribute("id");
        if (!id) {
          var base = slugify(el.textContent);
          id = base;
          var n = 1;
          while (used[id]) { id = base + "-" + n++; }
          el.setAttribute("id", id);
        }
        used[id] = true;
        // A hover "#" that copies a link to this section. Its glyph is a CSS
        // pseudo-element (see styles.css), so it stays out of textContent, which
        // the slug, the outline labels and find all read.
        var last = el.lastElementChild;
        if (!last || !last.classList.contains("heading-anchor")) {
          // Pin the heading's accessible name to its visible text, so the anchor
          // below does not append "Copy link…" to every heading a screen reader
          // announces. The anchor stays focusable and named in its own right.
          var text = el.textContent.trim();
          if (text && !el.hasAttribute("aria-label")) el.setAttribute("aria-label", text);
          var link = document.createElement("a");
          link.className = "heading-anchor";
          link.href = "#" + id;
          link.title = "Copy link to this section";
          link.setAttribute("aria-label", "Copy link to this section");
          el.appendChild(link);
        }
        found.push({ id: id, level: parseInt(el.tagName.slice(1), 10), el: el });
      });

      root.querySelectorAll("table").forEach(function (table) {
        if (table.parentElement && table.parentElement.classList.contains("table-scroll")) return;
        var wrap = document.createElement("div");
        wrap.className = "table-scroll";
        table.replaceWith(wrap);
        wrap.appendChild(table);
      });

      root.querySelectorAll("pre").forEach(function (pre) {
        if (pre.parentElement && pre.parentElement.classList.contains("code-block")) return;
        var code = pre.querySelector("code");
        var lang = "";
        if (code) {
          var cls = code.className || "";
          var m = cls.match(/language-([\w+#.-]+)/);
          if (m) lang = m[1];
        }

        var block = document.createElement("div");
        block.className = "code-block";
        var head = document.createElement("div");
        head.className = "code-head";
        var label = document.createElement("span");
        label.className = "code-lang";
        label.textContent = lang || "code";
        var copy = document.createElement("button");
        copy.type = "button";
        copy.className = "copy-btn";
        copy.textContent = "Copy";
        copy.addEventListener("click", function () {
          copyText(pre.innerText).then(function (ok) {
            copy.textContent = ok ? "Copied" : "Failed";
            toast(ok ? "Code copied" : "Clipboard unavailable");
            setTimeout(function () { copy.textContent = "Copy"; }, 1400);
          });
        });
        head.append(label, copy);
        pre.replaceWith(block);
        block.append(head, pre);
      });

      root.querySelectorAll("li").forEach(function (li) {
        if (li.querySelector(":scope > input[type=checkbox]")) {
          li.classList.add("task-list-item");
        }
      });

      return found;
    }

    /* Markdown usually references sibling images: serve those through Tauri's
     * asset protocol. The folder itself was scoped by Rust when it opened the
     * file, and the path keeps the platform's separator style because that is
     * what `convertFileSrc` (and the scope it is checked against) expects. */
    function rewriteLocalImages(doc) {
      var base = (doc.dir || "").replace(/[\\/]+$/, "");
      if (!base) return;

      var windows = /^[a-zA-Z]:/.test(base);
      var sep = windows ? "\\" : "/";
      var root = base.replace(/[\\/]/g, sep);

      docEl.querySelectorAll("img[src]").forEach(function (img) {
        var src = img.getAttribute("src") || "";
        if (!src) return;
        if (/^[a-zA-Z][a-zA-Z0-9+.-]*:/.test(src) || src.startsWith("//")) return;
        if (src.startsWith("/") || src.startsWith("\\")) return;

        var decoded = src;
        try { decoded = decodeURI(src); } catch (_) {}
        var cleaned = decoded.split(/[?#]/)[0].replace(/[\\/]/g, sep);
        if (!cleaned) return;

        var stacked = root + sep + cleaned;
        var parts = stacked.split(sep);
        var head = parts.shift();
        var out = [];
        parts.forEach(function (part) {
          if (!part || part === ".") return;
          if (part === "..") { out.pop(); return; }
          out.push(part);
        });
        var abs = (head ? head + sep : sep) + out.join(sep);
        if (windows && !/^[a-zA-Z]:[\\/]/.test(abs)) return;
        try { img.src = convertFileSrc(abs); } catch (_) {}
      });
    }

    function tocAnchor(item) {
      var a = document.createElement("a");
      a.href = "#" + item.id;
      a.className = "lvl-" + item.level;
      a.textContent = item.el.textContent.trim();
      a.title = a.textContent;
      return a;
    }

    function buildToc(items) {
      tocList.textContent = "";
      if (items.length < 2) {
        tocList.innerHTML = '<div class="toc-empty">No headings in this document.</div>';
        return;
      }
      var frag = document.createDocumentFragment();
      items.forEach(function (item) { frag.appendChild(tocAnchor(item)); });
      tocList.appendChild(frag);
    }

    /* Append the outline entries a progressive chunk contributed, from index
     * `from` on, without rebuilding the links already on screen. */
    function appendToc(items, from) {
      if (from >= items.length) return;
      if (tocList.querySelector(".toc-empty")) tocList.textContent = "";
      var frag = document.createDocumentFragment();
      for (var i = from; i < items.length; i++) frag.appendChild(tocAnchor(items[i]));
      tocList.appendChild(frag);
    }

    function scrollToId(id) {
      var el = document.getElementById(id);
      if (!el) return;
      var top = el.getBoundingClientRect().top + window.scrollY - (48 + 18);
      window.scrollTo({ top: top, behavior: "smooth" });
    }

    function updateHeader(doc) {
      document.title = doc.name + " — MD Viewer";
      $("doc-title").textContent = doc.name || "Untitled";
      $("doc-sub").textContent = doc.dir || "";
      var bits = [];
      if (doc.words) {
        bits.push(fmtInt(doc.words) + " words");
        bits.push(Math.max(1, Math.round(doc.words / 220)) + " min");
      }
      if (doc.truncated) bits.push("truncated");
      var meta = $("doc-meta");
      meta.textContent = bits.join(" · ");
      meta.title = doc.path || "";
    }

    function updateFooter(doc) {
      $("footer-words").textContent = doc.words ? fmtInt(doc.words) + " words" : "";
      var stamp = fmtStamp(doc.modified);
      var path = doc.path || "";
      $("footer-path").textContent = stamp ? path + "  ·  " + stamp : path;
    }

    /* Reading position per document: a long file reopens where it was left. The
     * map is capped so it cannot grow without bound. */
    function readPositions() {
      try {
        var stored = JSON.parse(localStorage.getItem(POSITION_KEY) || "{}");
        return stored && typeof stored === "object" ? stored : {};
      } catch (_) { return {}; }
    }

    var positionTimer = null;
    var positionDirty = false;

    function writePosition() {
      if (!current || !current.path) return;
      var map = readPositions();
      map[current.path] = Math.round(window.scrollY);
      var keys = Object.keys(map);
      if (keys.length > 60) {
        keys.slice(0, keys.length - 60).forEach(function (key) { delete map[key]; });
      }
      try { localStorage.setItem(POSITION_KEY, JSON.stringify(map)); } catch (_) {}
    }

    /* Leading edge plus trailing debounce: the first scroll of a burst is written
     * straight away (so closing the window right after scrolling cannot lose it),
     * and the resting position is written when the burst stops. */
    function schedulePositionSave() {
      if (!current || !current.path) return;
      if (!positionDirty) {
        positionDirty = true;
        writePosition();
      }
      clearTimeout(positionTimer);
      positionTimer = setTimeout(function () {
        writePosition();
        positionDirty = false;
      }, 400);
    }

    /* ---------- scroll: progress bar + active heading ---------- */

    var scrollQueued = false;
    function onScroll() {
      if (scrollQueued) return;
      scrollQueued = true;
      requestAnimationFrame(function () {
        scrollQueued = false;
        updateProgress();
        updateActiveHeading();
        schedulePositionSave();
      });
    }

    function updateProgress() {
      var height = document.documentElement.scrollHeight - window.innerHeight;
      var ratio = height > 40 ? Math.min(1, Math.max(0, window.scrollY / height)) : 0;
      progress.style.width = (ratio * 100).toFixed(2) + "%";
      progress.style.opacity = ratio > 0 ? "1" : "0";
    }

    var activeId = null;
    var headingTops = null;   // document offsets, measured once per layout

    /* Reading offsetTop for every heading on every scroll tick was the only part of
     * the UI that scaled linearly with document size (measured 0.75 ms per tick at
     * 500 headings, so ~7.5 ms at 5000). Measure once per layout, then binary
     * search: no layout reads and O(log n) per tick. */
    function measureHeadings() {
      headingTops = headings.map(function (item) { return item.el.offsetTop; });
    }

    function headingIndexAt(threshold) {
      if (!headingTops || headingTops.length !== headings.length) measureHeadings();
      var low = 0;
      var high = headingTops.length - 1;
      var found = -1;
      while (low <= high) {
        var mid = (low + high) >> 1;
        if (headingTops[mid] <= threshold) { found = mid; low = mid + 1; }
        else { high = mid - 1; }
      }
      return found;
    }

    function updateActiveHeading() {
      if (!headings.length) {
        setActiveHeadings([]);
        return;
      }
      var threshold = window.scrollY + 48 + 40;
      var index = headingIndexAt(threshold);
      var picked = index >= 0 ? headings[index] : headings[0];
      if (picked && picked.id !== activeId) setActiveHeadings([picked.id]);
    }

    function setActiveHeadings(ids) {
      activeId = ids.length ? ids[0] : null;
      tocList.querySelectorAll("a").forEach(function (a) {
        var on = ids.indexOf(a.getAttribute("href").slice(1)) !== -1;
        a.classList.toggle("active", on);
        // Screen readers get the same "you are here" signal as the highlight.
        if (on) a.setAttribute("aria-current", "location");
        else a.removeAttribute("aria-current");
      });
    }

    window.addEventListener("scroll", onScroll, { passive: true });
    window.addEventListener("resize", function () {
      headingTops = null;   // wrapping and widths change with the viewport
      onScroll();
    });

    /* ---------- contents drawer ---------- */

    var tocOpen = false;
    function openToc() {
      if (tocOpen) return;
      tocOpen = true;
      toc.hidden = false;
      tocScrim.hidden = false;
      requestAnimationFrame(function () { toc.classList.add("open"); });
      $("btn-toc").setAttribute("aria-expanded", "true");
      // Move focus into the panel so it is reachable by keyboard, and so Escape
      // has somewhere to come back from.
      var close = $("toc-close");
      if (close) close.focus();
    }

    function closeToc() {
      if (!tocOpen) return;
      tocOpen = false;
      toc.classList.remove("open");
      tocScrim.hidden = true;
      $("btn-toc").setAttribute("aria-expanded", "false");
      setTimeout(function () { if (!tocOpen) toc.hidden = true; }, 240);
      var inside = toc.contains(document.activeElement);
      if (inside || document.activeElement === document.body) $("btn-toc").focus();
    }

    function toggleToc() {
      if (tocOpen) closeToc(); else if (current) openToc();
    }

    /* ---------- opening documents ---------- */

    function openPath(path) {
      return invoke("load_path", { path: path })
        .then(function (doc) { if (doc) render(doc, { reset: true }); })
        .catch(function (error) { toast(String(error && error.message || error)); });
    }

    async function openDialog() {
      try {
        var doc = await invoke("open_dialog");
        if (doc) render(doc, { reset: true });
      } catch (error) {
        toast("Open dialog failed: " + String(error.message || error));
      }
    }

    async function reloadFile() {
      if (!current) { openDialog(); return; }
      try {
        var doc = await invoke("reload");
        if (doc) render(doc, { preserveScroll: true, notice: "Reloaded from disk" });
      } catch (error) {
        showDocError("Reload failed: " + String(error.message || error), true);
      }
    }

    /* Recents are only useful while the file is still there. Ask Rust which
     * paths still exist, drop the rest, and re-render. */
    function pruneRecent() {
      var items = readRecent();
      if (!items.length) return;
      invoke("filter_existing", { paths: items.map(function (item) { return item.path; }) })
        .then(function (kept) {
          if (!kept) return;
          var alive = Object.create(null);
          kept.forEach(function (path) { alive[path] = true; });
          var pruned = readRecent().filter(function (item) { return alive[item.path]; });
          if (pruned.length !== items.length) {
            try { localStorage.setItem(RECENT_KEY, JSON.stringify(pruned)); } catch (_) {}
            renderRecent();
          }
        })
        .catch(noop);
    }

    /* ---------- links ---------- */

    /* A heading's anchor copies a section link instead of navigating. Bound to the
     * article so it runs before the document-level handler below, and stops it
     * scrolling to the section the reader is already looking at. */
    docEl.addEventListener("click", function (event) {
      var link = event.target.closest && event.target.closest(".heading-anchor");
      if (!link) return;
      event.preventDefault();
      event.stopPropagation();
      var anchor = link.getAttribute("href").slice(1);
      var target = (current && current.path ? current.path : "") + "#" + anchor;
      copyText(target).then(function (ok) {
        toast(ok ? "Section link copied" : "Clipboard unavailable");
      });
    });

    document.addEventListener("click", async function (event) {
      var anchor = event.target.closest && event.target.closest("a[href]");
      if (!anchor) return;
      var href = anchor.getAttribute("href");
      if (!href || href.startsWith("#")) {
        if (href && href.length > 1) {
          event.preventDefault();
          scrollToId(decodeURIComponent(href.slice(1)));
        }
        return;
      }
      event.preventDefault();
      try {
        var result = await invoke("open_link", { href: href });
        if (!result) return;
        if (result.kind === "external") return;
        if (result.kind === "document" && result.doc) { render(result.doc, { reset: true }); return; }
        if (result.kind === "missing") toast("Not found: " + href);
      } catch (error) {
        toast("Could not follow link: " + String(error.message || error));
      }
    });

    /* ---------- find (CSS Custom Highlight API) ---------- */

    var find = (function () {
      var input = $("find-input");
      var count = $("find-count");
      var bar = $("findbar");
      var ranges = [];
      var index = -1;
      var total = 0;
      var HIGHLIGHT_CAP = 1500;
      var options = readOptions();
      var supported = typeof window.Highlight === "function" &&
        !!(window.CSS && CSS.highlights);

      function readOptions() {
        try {
          var raw = JSON.parse(localStorage.getItem("mdviewer.find") || "{}");
          return { caseSensitive: !!raw.caseSensitive, wholeWord: !!raw.wholeWord };
        } catch (_) { return { caseSensitive: false, wholeWord: false }; }
      }

      function saveOptions() {
        try { localStorage.setItem("mdviewer.find", JSON.stringify(options)); } catch (_) {}
      }

      function escapeRegExp(text) {
        return text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
      }

      function buildMatcher(term) {
        var flags = options.caseSensitive ? "g" : "gi";
        var pattern = escapeRegExp(term);
        if (options.wholeWord) pattern = "\\b" + pattern + "\\b";
        return new RegExp(pattern, flags);
      }
      var matchHighlight = supported ? new Highlight() : null;
      var currentHighlight = supported ? new Highlight() : null;

      if (supported) {
        CSS.highlights.set("find-match", matchHighlight);
        CSS.highlights.set("find-current", currentHighlight);
      }

      function clear() {
        ranges = [];
        index = -1;
        total = 0;
        if (supported) { matchHighlight.clear(); currentHighlight.clear(); }
        count.textContent = "0/0";
        count.title = "";
      }

      /* Counts every match but only builds Range objects for the first
       * HIGHLIGHT_CAP, so a pathological document cannot freeze the UI while the
       * counter still tells the truth. */
      function collect(term) {
        clear();
        if (!term || term.length < 2 || docEl.hidden) return;

        var matcher;
        try { matcher = buildMatcher(term); } catch (_) { return; }

        var walker = document.createTreeWalker(docEl, NodeFilter.SHOW_TEXT, {
          acceptNode: function (node) {
            if (!node.nodeValue || !node.nodeValue.trim()) return NodeFilter.FILTER_REJECT;
            var parent = node.parentElement;
            if (parent && parent.closest("script, style")) return NodeFilter.FILTER_REJECT;
            return NodeFilter.FILTER_ACCEPT;
          }
        });
        var node;
        while ((node = walker.nextNode())) {
          var text = node.nodeValue;
          var match;
          matcher.lastIndex = 0;
          while ((match = matcher.exec(text))) {
            total += 1;
            if (ranges.length < HIGHLIGHT_CAP) {
              var range = document.createRange();
              range.setStart(node, match.index);
              range.setEnd(node, match.index + match[0].length);
              ranges.push(range);
            }
            if (match[0].length === 0) matcher.lastIndex += 1; // zero-width safety
          }
        }
        if (supported) ranges.forEach(function (range) { matchHighlight.add(range); });
        if (total > ranges.length) {
          count.title = "Highlighting the first " + ranges.length + " of " + total + " matches";
        }
        count.textContent = total ? "1/" + total : "0/0";
        if (ranges.length) go(0);
      }

      function go(next) {
        if (!ranges.length) return;
        index = ((next % ranges.length) + ranges.length) % ranges.length;
        var range = ranges[index];
        if (supported) {
          currentHighlight.clear();
          currentHighlight.add(range);
        }
        count.textContent = (index + 1) + "/" + total;
        var rect = range.getBoundingClientRect();
        var target = rect.top + window.scrollY - window.innerHeight / 2;
        window.scrollTo({ top: target, behavior: "smooth" });
      }

      function open() {
        if (bar.hidden) lastFocus = document.activeElement;
        bar.hidden = false;
        input.focus();
        input.select();
        if (input.value) collect(input.value);
      }

      function close() {
        bar.hidden = true;
        clear();
        // Give focus back to whatever had it (usually the article or a button).
        if (lastFocus && document.contains(lastFocus) && typeof lastFocus.focus === "function") {
          lastFocus.focus();
        } else {
          docEl.focus();
        }
        lastFocus = null;
      }

      var debounce = null;
      var lastFocus = null;

      var caseBtn = $("find-case");
      var wordBtn = $("find-word");

      function syncToggles() {
        caseBtn.setAttribute("aria-pressed", String(options.caseSensitive));
        wordBtn.setAttribute("aria-pressed", String(options.wholeWord));
        caseBtn.title = "Match case (Alt+C)";
        wordBtn.title = "Whole word (Alt+W)";
      }

      function rerun() {
        if (input.value.trim()) collect(input.value.trim());
      }

      function toggleCase() {
        options.caseSensitive = !options.caseSensitive;
        saveOptions();
        syncToggles();
        rerun();
        input.focus();
      }

      function toggleWord() {
        options.wholeWord = !options.wholeWord;
        saveOptions();
        syncToggles();
        rerun();
        input.focus();
      }

      caseBtn.addEventListener("click", toggleCase);
      wordBtn.addEventListener("click", toggleWord);
      syncToggles();

      input.addEventListener("input", function () {
        clearTimeout(debounce);
        debounce = setTimeout(function () { collect(input.value.trim()); }, 140);
      });

      input.addEventListener("keydown", function (event) {
        if (event.altKey && (event.key === "c" || event.key === "C")) {
          event.preventDefault();
          toggleCase();
          return;
        }
        if (event.altKey && (event.key === "w" || event.key === "W")) {
          event.preventDefault();
          toggleWord();
          return;
        }
        if (event.key === "Enter") {
          event.preventDefault();
          if (!ranges.length) collect(input.value.trim());
          else go(index + (event.shiftKey ? -1 : 1));
        } else if (event.key === "Escape") {
          event.preventDefault();
          close();
        }
      });

      $("find-next").addEventListener("click", function () { go(index + 1); });
      $("find-prev").addEventListener("click", function () { go(index - 1); });
      $("find-close").addEventListener("click", close);

      return {
        open: open,
        close: close,
        isOpen: function () { return !bar.hidden; },
        refresh: function () {
          if (!bar.hidden && input.value.trim()) collect(input.value.trim());
          else clear();
        },
        supported: supported
      };
    })();

    /* ---------- theme cycling ---------- */

    var order = ["auto", "light", "dark"];
    var themeLabels = { auto: "system", light: "light", dark: "dark" };

    function cycleTheme() {
      themePref = order[(order.indexOf(themePref) + 1) % order.length];
      localStorage.setItem(THEME_KEY, themePref);
      applyTheme();
      $("btn-theme").title = "Theme: " + themeLabels[themePref] + " (Ctrl+Shift+L)";
      $("btn-theme").setAttribute("aria-label", "Theme: " + themeLabels[themePref] + " (click to change)");
      toast("Theme: " + themeLabels[themePref]);
    }

    /* ---------- keyboard ---------- */

    document.addEventListener("keydown", function (event) {
      var mod = event.ctrlKey || event.metaKey;
      var target = event.target;
      var typing = !!target && (target.tagName === "INPUT" || target.tagName === "TEXTAREA" || target.isContentEditable);

      if (event.key === "Escape") {
        if (find.isOpen()) { find.close(); return; }
        if (tocOpen) { closeToc(); return; }
      }
      if (event.key === "F5" || (mod && !event.shiftKey && event.key.toLowerCase() === "r")) {
        event.preventDefault();
        reloadFile();
        return;
      }
      if (mod && event.key.toLowerCase() === "o") {
        event.preventDefault();
        openDialog();
        return;
      }
      if (mod && event.key.toLowerCase() === "f") {
        event.preventDefault();
        find.open();
        return;
      }
      if (mod && event.key.toLowerCase() === "b") {
        event.preventDefault();
        toggleToc();
        return;
      }
      if (mod && event.shiftKey && event.key.toLowerCase() === "l") {
        event.preventDefault();
        cycleTheme();
        return;
      }
      if (mod && (event.key === "=" || event.key === "+" || event.key === "-" || event.key === "0")) {
        event.preventDefault();
        if (event.key === "0") readingScale = applyScale(1);
        else readingScale = applyScale(readingScale + (event.key === "-" ? -0.1 : 0.1));
        return;
      }
      if (!mod) {
        // Never hijack plain letters while the user is typing (the find field).
        if (typing) return;
        var key = event.key.toLowerCase();
        if (key === "j" || key === "k") {
          event.preventDefault();
          window.scrollBy({
            top: (key === "j" ? 1 : -1) * Math.round(window.innerHeight * 0.42),
            behavior: "smooth"
          });
        }
      }
    });

    /* ---------- wire the chrome ---------- */

    $("btn-open").addEventListener("click", openDialog);
    $("welcome-open").addEventListener("click", openDialog);
    $("welcome-reopen").addEventListener("click", function () {
      var last = readRecent()[0];
      if (last && last.path) openPath(last.path);
    });
    $("error-retry").addEventListener("click", reloadFile);
    $("recent-clear").addEventListener("click", function () {
      try { localStorage.removeItem(RECENT_KEY); } catch (_) {}
      renderRecent();
      toast("Recent list cleared");
    });

    /* ---------- "set as default" ---------- */

    function refreshDefaultStatus() {
      var status = $("default-status");
      invoke("default_status")
        .then(function (state) {
          if (!state || !state.supported) {
            $("welcome-default").hidden = true;
            status.hidden = true;
            return;
          }
          status.hidden = false;
          if (state.isDefault) {
            status.textContent = "MD Viewer is the default app for .md files.";
            status.classList.add("is-default");
            $("welcome-default").textContent = "Re-register the association";
          } else {
            status.textContent = "Not the default app for .md files yet.";
            status.classList.remove("is-default");
            $("welcome-default").textContent = "Set as default for .md files";
          }
        })
        .catch(function () {
          $("welcome-default").hidden = true;
          status.hidden = true;
        });
    }

    $("welcome-default").addEventListener("click", function () {
      var button = this;
      button.disabled = true;
      invoke("make_default")
        .then(function (report) {
          toast(report && report.message ? report.message : "Registered.");
          // Windows asks in its own dialog; re-read the state a few times so the
          // label catches up once the user has finished choosing.
          [1200, 4000, 9000].forEach(function (delay) {
            setTimeout(refreshDefaultStatus, delay);
          });
        })
        .catch(function (error) {
          toast("Could not register: " + String((error && error.message) || error));
        })
        .finally(function () {
          setTimeout(function () { button.disabled = false; }, 800);
        });
    });
    $("btn-toc").addEventListener("click", toggleToc);
    $("toc-close").addEventListener("click", closeToc);
    tocScrim.addEventListener("click", closeToc);
    // One delegated listener instead of 500 per-link ones.
    tocList.addEventListener("click", function (event) {
      var link = event.target.closest && event.target.closest("a[href^='#']");
      if (!link) return;
      event.preventDefault();
      scrollToId(link.getAttribute("href").slice(1));
      if (window.matchMedia("(max-width: 1100px)").matches) closeToc();
    });
    $("btn-theme").addEventListener("click", cycleTheme);
    $("btn-theme").title = "Theme: " + themeLabels[themePref] + " (Ctrl+Shift+L)";
    $("btn-theme").setAttribute("aria-label", "Theme: " + themeLabels[themePref] + " (click to change)");
    $("skip-link").addEventListener("click", function (event) {
      event.preventDefault();
      docEl.focus();
      docEl.scrollIntoView({ block: "start", behavior: "smooth" });
    });

    function setDragOverlay(active) {
      if (active) { dropzone.hidden = false; } else { dropzone.hidden = true; }
    }

    /* ---------- Tauri events ---------- */

    function subscribe() {
      listen("md://open", function (event) {
        if (event.payload) render(event.payload, { reset: true });
      });
      listen("md://reload", function (event) {
        if (event.payload) render(event.payload, { preserveScroll: true, notice: "Reloaded from disk" });
      });
      listen("md://error", function (event) {
        var message = (event.payload && event.payload.message) || "Unknown error";
        if (current) showDocError(message, true); else { welcomeError.textContent = message; welcomeError.hidden = false; }
        toast(message);
      });
      listen("md://notice", function (event) {
        toast((event.payload && event.payload.message) || "");
      });
      listen("md://drag", function (event) {
        setDragOverlay(!!(event.payload && event.payload.active));
      });
    }

    /* ---------- startup ---------- */

    renderRecent();
    pruneRecent();
    subscribe();
    refreshDefaultStatus();

    invoke("initial_document")
      .then(function (doc) {
        if (doc) render(doc, { reset: true });
        else showWelcome();
      })
      .catch(function (error) {
        showWelcome();
        welcomeError.textContent = "Startup failed: " + String(error && error.message || error);
        welcomeError.hidden = false;
      })
      .finally(function () {
        // Tell Rust the first paint is done so it can show the window without a flash.
        invoke("ready").catch(noop);
      });
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", boot);
  } else {
    boot();
  }
})();
