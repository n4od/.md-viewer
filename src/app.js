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

  /* ------------------------------------------------------------------ *
   * Theme — resolved before first paint because this script is in <head>
   * ------------------------------------------------------------------ */

  var THEME_KEY = "mdviewer.theme";
  var SCALE_KEY = "mdviewer.scale";
  var RECENT_KEY = "mdviewer.recent";
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
    return String(text)
      .trim()
      .toLowerCase()
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

    function showWelcome() {
      welcome.hidden = false;
      docEl.hidden = true;
      footer.hidden = true;
      errorCard.hidden = true;
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

    function showDocError(message) {
      errorCard.hidden = false;
      errorCard.textContent = message;
    }

    /* ---------- document rendering ---------- */

    function render(doc, options) {
      options = options || {};
      if (!doc) { showWelcome(); return; }
      if (doc.error) {
        var message = "Could not open " + (doc.path || "file") + "\n" + doc.error;
        if (current && !options.reset) {
          showDocError(message);
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

      docEl.innerHTML = doc.html || "";
      headings = enhance(docEl);
      buildToc(headings);
      updateHeader(doc);
      updateFooter(doc);
      rewriteLocalImages(doc);

      if (options.preserveScroll && sameFile) {
        window.scrollTo({ top: previousScroll, behavior: "instant" });
      } else {
        window.scrollTo({ top: 0, behavior: "instant" });
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

    function showWelcomeState() {
      welcome.hidden = false;
      docEl.hidden = true;
      footer.hidden = true;
      docEl.textContent = "";
      current = null;
    }

    /* Add ids to headings, wrap tables/pre and give code blocks a toolbar. */
    function enhance(root) {
      var found = [];
      var used = Object.create(null);

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

    function buildToc(items) {
      tocList.textContent = "";
      if (items.length < 2) {
        tocList.innerHTML = '<div class="toc-empty">No headings in this document.</div>';
        return;
      }
      var frag = document.createDocumentFragment();
      items.forEach(function (item) {
        var a = document.createElement("a");
        a.href = "#" + item.id;
        a.className = "lvl-" + item.level;
        a.textContent = item.el.textContent.trim();
        a.title = a.textContent;
        a.addEventListener("click", function (event) {
          event.preventDefault();
          scrollToId(item.id);
          if (window.matchMedia("(max-width: 1100px)").matches) closeToc();
        });
        frag.appendChild(a);
      });
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

    /* ---------- scroll: progress bar + active heading ---------- */

    var scrollQueued = false;
    function onScroll() {
      if (scrollQueued) return;
      scrollQueued = true;
      requestAnimationFrame(function () {
        scrollQueued = false;
        updateProgress();
        updateActiveHeading();
      });
    }

    function updateProgress() {
      var height = document.documentElement.scrollHeight - window.innerHeight;
      var ratio = height > 40 ? Math.min(1, Math.max(0, window.scrollY / height)) : 0;
      progress.style.width = (ratio * 100).toFixed(2) + "%";
      progress.style.opacity = ratio > 0 ? "1" : "0";
    }

    var activeId = null;
    function updateActiveHeading() {
      if (!headings.length) {
        setActiveHeadings([]);
        return;
      }
      var threshold = window.scrollY + 48 + 40;
      var picked = null;
      for (var i = 0; i < headings.length; i++) {
        if (headings[i].el.offsetTop <= threshold) picked = headings[i]; else break;
      }
      if (!picked) picked = headings[0];
      if (picked && picked.id !== activeId) setActiveHeadings([picked.id]);
    }

    function setActiveHeadings(ids) {
      activeId = ids.length ? ids[0] : null;
      tocList.querySelectorAll("a").forEach(function (a) {
        var on = ids.indexOf(a.getAttribute("href").slice(1)) !== -1;
        a.classList.toggle("active", on);
      });
    }

    window.addEventListener("scroll", onScroll, { passive: true });
    window.addEventListener("resize", onScroll);

    /* ---------- contents drawer ---------- */

    function openToc() {
      toc.hidden = false;
      tocScrim.hidden = false;
      requestAnimationFrame(function () { toc.classList.add("open"); });
      $("btn-toc").setAttribute("aria-pressed", "true");
    }

    function closeToc() {
      toc.classList.remove("open");
      tocScrim.hidden = true;
      $("btn-toc").setAttribute("aria-pressed", "false");
      setTimeout(function () { if (!toc.classList.contains("open")) toc.hidden = true; }, 240);
    }

    function toggleToc() {
      if (toc.classList.contains("open")) closeToc(); else if (current) openToc();
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
        showDocError("Reload failed: " + String(error.message || error));
      }
    }

    /* ---------- links ---------- */

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
      var supported = typeof window.Highlight === "function" &&
        !!(window.CSS && CSS.highlights);
      var matchHighlight = supported ? new Highlight() : null;
      var currentHighlight = supported ? new Highlight() : null;

      if (supported) {
        CSS.highlights.set("find-match", matchHighlight);
        CSS.highlights.set("find-current", currentHighlight);
      }

      function clear() {
        ranges = [];
        index = -1;
        if (supported) { matchHighlight.clear(); currentHighlight.clear(); }
        count.textContent = "0/0";
      }

      function collect(term) {
        clear();
        if (!term || term.length < 2 || docEl.hidden) return;
        var needle = term.toLowerCase();
        var walker = document.createTreeWalker(docEl, NodeFilter.SHOW_TEXT, {
          acceptNode: function (node) {
            if (!node.nodeValue || !node.nodeValue.trim()) return NodeFilter.FILTER_REJECT;
            var parent = node.parentElement;
            if (parent && parent.closest("script, style")) return NodeFilter.FILTER_REJECT;
            return NodeFilter.FILTER_ACCEPT;
          }
        });
        var node;
        var limit = 1200;
        while ((node = walker.nextNode()) && ranges.length < limit) {
          var text = node.nodeValue.toLowerCase();
          var from = 0;
          while (ranges.length < limit) {
            var at = text.indexOf(needle, from);
            if (at === -1) break;
            var range = document.createRange();
            range.setStart(node, at);
            range.setEnd(node, at + needle.length);
            ranges.push(range);
            from = at + needle.length;
          }
        }
        if (supported) ranges.forEach(function (range) { matchHighlight.add(range); });
        count.textContent = ranges.length ? "1/" + ranges.length : "0/0";
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
        count.textContent = (index + 1) + "/" + ranges.length;
        var rect = range.getBoundingClientRect();
        var target = rect.top + window.scrollY - window.innerHeight / 2;
        window.scrollTo({ top: target, behavior: "smooth" });
      }

      function open() {
        bar.hidden = false;
        input.focus();
        input.select();
        if (input.value) collect(input.value);
      }

      function close() {
        bar.hidden = true;
        clear();
        if (current) docEl.focus();
      }

      var debounce = null;
      input.addEventListener("input", function () {
        clearTimeout(debounce);
        debounce = setTimeout(function () { collect(input.value.trim()); }, 140);
      });

      input.addEventListener("keydown", function (event) {
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
      toast("Theme: " + themeLabels[themePref]);
    }

    /* ---------- keyboard ---------- */

    document.addEventListener("keydown", function (event) {
      var mod = event.ctrlKey || event.metaKey;

      if (event.key === "Escape") {
        if (find.isOpen()) { find.close(); return; }
        if (toc.classList.contains("open")) { closeToc(); return; }
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
    $("btn-theme").addEventListener("click", cycleTheme);
    $("btn-theme").title = "Theme: " + themeLabels[themePref] + " (Ctrl+Shift+L)";

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
        if (current) showDocError(message); else { welcomeError.textContent = message; welcomeError.hidden = false; }
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
