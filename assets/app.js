"use strict";
(() => {
  const $ = (sel) => document.querySelector(sel);
  const el = (tag, cls, text) => {
    const e = document.createElement(tag);
    if (cls) e.className = cls;
    if (text != null) e.textContent = text;
    return e;
  };

  // ---------- icons (16px, stroke based) ----------
  const svg = (d, size = 16) =>
    `<svg width="${size}" height="${size}" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">${d}</svg>`;
  const ICON = {
    sidebar: svg('<rect x="1.75" y="2.25" width="12.5" height="11.5" rx="2"/><path d="M6 2.5v11"/>'),
    back: svg('<path d="M10 3L5 8l5 5"/>'),
    fwd: svg('<path d="M6 3l5 5-5 5"/>'),
    up: svg('<path d="M8 13V3M3.5 7.5L8 3l4.5 4.5"/>'),
    refresh: svg('<path d="M13.5 8a5.5 5.5 0 1 1-1.6-3.9"/><path d="M13.5 2.5v3h-3"/>'),
    eye: svg('<path d="M1.5 8s2.4-4.5 6.5-4.5S14.5 8 14.5 8 12.1 12.5 8 12.5 1.5 8 1.5 8z"/><circle cx="8" cy="8" r="1.8"/>'),
    sun: svg('<circle cx="8" cy="8" r="3"/><path d="M8 1v1.5M8 13.5V15M1 8h1.5M13.5 8H15M3 3l1 1M12 12l1 1M3 13l1-1M12 4l1-1"/>'),
    moon: svg('<path d="M13.5 9.5A5.5 5.5 0 0 1 6.5 2.5a5.5 5.5 0 1 0 7 7z"/>'),
    code: svg('<path d="M5.5 4L1.5 8l4 4M10.5 4l4 4-4 4"/>'),
    search: svg('<circle cx="7" cy="7" r="4.5"/><path d="M10.5 10.5L14 14"/>'),
    close: svg('<path d="M4 4l8 8M12 4l-8 8"/>'),
    chevUp: svg('<path d="M3.5 10L8 5.5l4.5 4.5"/>'),
    chevDown: svg('<path d="M3.5 6L8 10.5 12.5 6"/>'),
    caret: svg('<path d="M6 4l4 4-4 4"/>'),
    folder: svg('<path d="M1.75 4.25a1 1 0 0 1 1-1h3.5l1.5 1.5h5.5a1 1 0 0 1 1 1v6.5a1 1 0 0 1-1 1H2.75a1 1 0 0 1-1-1z"/>'),
    md: svg('<path d="M3.75 1.75h6l3.5 3.5v8a1 1 0 0 1-1 1h-8.5a1 1 0 0 1-1-1v-10.5a1 1 0 0 1 1-1z"/><path d="M9.5 1.75v3.5h3.5"/><path d="M5 12V8.5l1.5 1.75L8 8.5V12"/><path d="M10.5 8.5V12M9.25 10.75l1.25 1.25 1.25-1.25"/>', 16),
    file: svg('<path d="M3.75 1.75h6l3.5 3.5v8a1 1 0 0 1-1 1h-8.5a1 1 0 0 1-1-1v-10.5a1 1 0 0 1 1-1z"/><path d="M9.5 1.75v3.5h3.5"/>'),
  };

  // ---------- persisted settings ----------
  const store = {
    get(k, d) {
      try { const v = localStorage.getItem("mdview." + k); return v == null ? d : JSON.parse(v); } catch { return d; }
    },
    set(k, v) {
      try { localStorage.setItem("mdview." + k, JSON.stringify(v)); } catch { /* ignore */ }
    },
  };

  const S = {
    sep: "/",
    root: "",
    file: null,
    history: [],
    hIndex: -1,
    showTree: store.get("showTree", true),
    showSource: store.get("showSource", false),
    showAll: store.get("showAll", false),
    theme: store.get("theme", "auto"), // auto | light | dark
    zoom: store.get("zoom", 1),
    treeW: store.get("treeW", 260),
    sourceW: store.get("sourceW", null),
    sourceLoadedFor: null,
  };

  const ipc = (cmd, arg = "") => {
    try { window.ipc.postMessage(JSON.stringify({ cmd, arg })); } catch { /* not in app */ }
  };

  async function api(name, params) {
    const r = await fetch(`api/${name}?${new URLSearchParams(params)}`);
    if (!r.ok) throw new Error(await r.text());
    return r.json();
  }

  // ---------- path helpers ----------
  const basename = (p) => p.split(/[\\/]/).filter(Boolean).pop() || p;
  const dirname = (p) => {
    const i = Math.max(p.lastIndexOf("/"), p.lastIndexOf("\\"));
    return i <= 0 ? p.slice(0, i + 1) || p : p.slice(0, i);
  };
  const samePath = (a, b) => a && b && (S.sep === "\\" ? a.toLowerCase() === b.toLowerCase() : a === b);
  const isUnder = (p, dir) => {
    const d = dir.endsWith(S.sep) ? dir : dir + S.sep;
    return S.sep === "\\" ? p.toLowerCase().startsWith(d.toLowerCase()) : p.startsWith(d);
  };

  // ---------- theme / zoom / layout ----------
  const media = window.matchMedia("(prefers-color-scheme: dark)");
  function applyTheme() {
    const t = S.theme === "auto" ? (media.matches ? "dark" : "light") : S.theme;
    document.documentElement.dataset.theme = t;
    $("#btn-theme").innerHTML = t === "dark" ? ICON.sun : ICON.moon;
    if (document.querySelector(".mermaid[data-done]")) renderMermaid(true);
  }
  media.addEventListener?.("change", applyTheme);

  function toggleTheme() {
    const cur = document.documentElement.dataset.theme;
    S.theme = cur === "dark" ? "light" : "dark";
    store.set("theme", S.theme);
    applyTheme();
  }

  function applyZoom() {
    document.documentElement.style.setProperty("--zoom", S.zoom);
    store.set("zoom", S.zoom);
  }

  function applyLayout() {
    document.body.classList.toggle("no-tree", !S.showTree);
    document.body.classList.toggle("no-source", !S.showSource);
    $("#btn-tree").classList.toggle("on", S.showTree);
    $("#btn-source").classList.toggle("on", S.showSource);
    $("#btn-all").classList.toggle("on", S.showAll);
    document.documentElement.style.setProperty("--tree-w", S.treeW + "px");
    if (S.sourceW) document.documentElement.style.setProperty("--source-w", S.sourceW + "px");
    if (S.showSource) loadSource();
  }

  function toggleTree() { S.showTree = !S.showTree; store.set("showTree", S.showTree); applyLayout(); }
  function toggleSource() { S.showSource = !S.showSource; store.set("showSource", S.showSource); applyLayout(); syncFromPreview(); }

  function splitter(id, onMove) {
    const s = $(id);
    s.addEventListener("mousedown", (e) => {
      e.preventDefault();
      s.classList.add("drag");
      document.body.classList.add("resizing");
      const move = (ev) => onMove(ev.clientX);
      const up = () => {
        s.classList.remove("drag");
        document.body.classList.remove("resizing");
        window.removeEventListener("mousemove", move);
        window.removeEventListener("mouseup", up);
      };
      window.addEventListener("mousemove", move);
      window.addEventListener("mouseup", up);
    });
  }
  splitter("#split-left", (x) => {
    S.treeW = Math.max(140, Math.min(x, window.innerWidth - 300));
    store.set("treeW", S.treeW);
    applyLayout();
    fadeCrumbs();
  });
  splitter("#split-right", (x) => {
    S.sourceW = Math.max(160, Math.min(window.innerWidth - x, window.innerWidth - 300));
    store.set("sourceW", S.sourceW);
    applyLayout();
  });

  // ---------- file tree ----------
  const tree = $("#tree");

  function setRoot(dir) {
    S.root = dir;
    renderCrumbs();
    loadTree();
  }

  function renderCrumbs() {
    const c = $("#crumbs");
    c.innerHTML = "";
    const inner = el("span", "inner");
    const parts = S.root.split(/[\\/]/);
    let acc = "";
    parts.forEach((part, i) => {
      if (i === 0) acc = part + S.sep; // "/" on unix, "C:\" on windows
      else acc = acc.endsWith(S.sep) ? acc + part : acc + S.sep + part;
      if (!part && i > 0) return;
      const target = acc;
      const a = el("a", null, part || S.sep);
      a.title = target;
      a.onclick = () => setRoot(target);
      if (inner.childNodes.length) inner.append(el("span", "sep", "›"));
      inner.append(a);
    });
    c.append(inner);
    c.title = S.root;
    fadeCrumbs();
    $("#btn-up").disabled = dirname(S.root) === S.root;
  }

  // Long paths are clipped on the left; fade the cut edge instead of slicing a word.
  function fadeCrumbs() {
    const c = $("#crumbs");
    c.classList.toggle("overflow", c.scrollWidth > c.clientWidth + 1);
  }
  window.addEventListener("resize", fadeCrumbs);

  async function loadTree() {
    tree.innerHTML = "";
    await fillDir(tree, S.root, 0);
    if (S.file) await revealInTree(S.file);
  }

  async function fillDir(ul, dir, depth) {
    let data;
    try {
      data = await api("list", { path: dir, all: S.showAll ? 1 : 0 });
    } catch (e) {
      ul.innerHTML = "";
      const li = el("li", "empty", String(e.message || e));
      li.style.setProperty("--depth", depth);
      ul.append(li);
      return;
    }
    ul.innerHTML = "";
    ul.dataset.path = data.path;
    if (!data.entries.length) {
      const li = el("li", "empty", S.showAll ? "Empty folder" : "No Markdown files");
      li.style.setProperty("--depth", depth);
      ul.append(li);
    }
    for (const ent of data.entries) ul.append(treeNode(ent, depth));
  }

  function treeNode(ent, depth) {
    const li = el("li", ent.dir ? "dir" : "file");
    li.dataset.path = ent.path;
    const row = el("div", "row" + (!ent.dir && !ent.md ? " other" : ""));
    row.style.setProperty("--depth", depth);
    row.title = ent.path;
    const caret = el("span", "caret");
    if (ent.dir) caret.innerHTML = ICON.caret;
    const ico = el("span", "fico");
    ico.innerHTML = ent.dir ? ICON.folder : ent.md ? ICON.md : ICON.file;
    row.append(caret, ico, el("span", "name", ent.name));
    li.append(row);
    if (!ent.dir && samePath(ent.path, S.file)) row.classList.add("current");

    if (ent.dir) {
      row.onclick = () => toggleDir(li, depth);
      row.ondblclick = (e) => { e.preventDefault(); setRoot(ent.path); };
    } else if (ent.md) {
      row.onclick = () => openFile(ent.path);
    } else {
      row.ondblclick = () => ipc("open_path", ent.path);
      row.title = ent.path + "\n(double-click to open with the default app)";
    }
    return li;
  }

  async function toggleDir(li, depth, forceOpen = false) {
    let sub = li.querySelector(":scope > ul");
    if (li.classList.contains("open") && !forceOpen) {
      li.classList.remove("open");
      if (sub) sub.hidden = true;
      return;
    }
    li.classList.add("open");
    if (!sub) {
      sub = el("ul");
      li.append(sub);
      await fillDir(sub, li.dataset.path, depth + 1);
    }
    sub.hidden = false;
  }

  async function revealInTree(file) {
    if (!isUnder(file, S.root)) return;
    const rel = file.slice(S.root.length).split(/[\\/]/).filter(Boolean);
    let ul = tree;
    let path = S.root;
    for (let i = 0; i < rel.length - 1; i++) {
      path = path.endsWith(S.sep) ? path + rel[i] : path + S.sep + rel[i];
      const li = [...ul.children].find((c) => c.classList.contains("dir") && samePath(c.dataset.path, path));
      if (!li) return;
      await toggleDir(li, i, true);
      ul = li.querySelector(":scope > ul");
    }
    markCurrent();
  }

  function markCurrent() {
    let cur = null;
    tree.querySelectorAll("li.file > .row").forEach((r) => {
      const on = samePath(r.parentElement.dataset.path, S.file);
      r.classList.toggle("current", on);
      if (on) cur = r;
    });
    if (cur) {
      const t = tree.getBoundingClientRect();
      const r = cur.getBoundingClientRect();
      if (r.top < t.top || r.bottom > t.bottom) cur.scrollIntoView({ block: "center" });
    }
  }

  async function refreshDir(dir) {
    const ul = samePath(dir, S.root) ? tree : [...tree.querySelectorAll("ul")].find((u) => samePath(u.dataset.path, dir));
    if (!ul) return;
    const open = new Set([...ul.querySelectorAll("li.dir.open")].map((l) => l.dataset.path));
    const scroll = tree.scrollTop;
    const depth = ul === tree ? 0 : Number(ul.parentElement.querySelector(":scope > .row").style.getPropertyValue("--depth")) + 1;
    await fillDir(ul, dir, depth);
    for (const li of ul.querySelectorAll(":scope > li.dir")) {
      if (open.has(li.dataset.path)) await toggleDir(li, depth, true);
    }
    tree.scrollTop = scroll;
    markCurrent();
  }

  // ---------- document ----------
  const preview = $("#preview");
  const previewPane = $("#preview-pane");

  function showState(title, detail, isError) {
    preview.innerHTML = "";
    const box = el("div", "state" + (isError ? " error" : ""));
    box.append(el("h2", null, title));
    if (detail) box.append(el("p", null, detail));
    preview.append(box);
  }

  function setDocPath(path) {
    const p = $("#doc-path");
    p.innerHTML = "";
    if (!path) return;
    const span = el("span");
    span.style.direction = "ltr";
    span.style.unicodeBidi = "isolate";
    span.append(document.createTextNode(dirname(path) + S.sep), el("b", null, basename(path)));
    p.append(span);
    p.title = path;
  }

  async function openFile(path, opts = {}) {
    const { push = true, anchor = null, keepScroll = false } = opts;
    let doc;
    try {
      doc = await api("render", { path });
    } catch (e) {
      showState("Could not open file", String(e.message || e), true);
      return;
    }
    const changed = !samePath(doc.path, S.file);
    const scroll = previewPane.scrollTop;
    S.file = doc.path;
    if (push && changed) {
      S.history = S.history.slice(0, S.hIndex + 1);
      S.history.push(doc.path);
      S.hIndex = S.history.length - 1;
    }
    updateNav();
    preview.innerHTML = doc.html;
    enhance();
    setDocPath(doc.path);
    ipc("title", `${doc.name} — mdview`);
    ipc("watch", doc.dir);

    if (!isUnder(doc.path, S.root) && changed) setRoot(doc.dir);
    else if (changed) revealInTree(doc.path);

    if (keepScroll) previewPane.scrollTop = scroll;
    else if (anchor) scrollToAnchor(anchor);
    else previewPane.scrollTop = 0;

    S.sourceLoadedFor = null;
    if (S.showSource) await loadSource(keepScroll);
  }

  function updateNav() {
    $("#btn-back").disabled = S.hIndex <= 0;
    $("#btn-fwd").disabled = S.hIndex >= S.history.length - 1;
  }
  function go(delta) {
    const i = S.hIndex + delta;
    if (i < 0 || i >= S.history.length) return;
    S.hIndex = i;
    openFile(S.history[i], { push: false });
  }

  const EXTERNAL = /^[a-z][a-z0-9+.-]*:/i;
  const LOCAL_URL = /^(data:|blob:|mdview:|https?:\/\/mdview\.localhost)/i;

  function localUrl(rel) {
    return `fs?${new URLSearchParams({ base: S.file || S.root, rel })}`;
  }

  function enhance() {
    // Local images and media resolve relative to the document.
    for (const node of preview.querySelectorAll("img[src], video[src], audio[src], source[src]")) {
      const src = node.getAttribute("src");
      if (!src || LOCAL_URL.test(src) || /^https?:|^\/\//i.test(src)) continue;
      node.setAttribute("src", localUrl(src.replace(/^file:\/\//i, "")));
    }
    for (const node of preview.querySelectorAll("source[srcset], img[srcset]")) {
      const src = node.getAttribute("srcset");
      if (src && !/^https?:|^\/\/|^data:/i.test(src)) node.setAttribute("srcset", localUrl(src.split(/\s+/)[0]));
    }
    // Code copy buttons.
    for (const pre of preview.querySelectorAll("pre:not(.mermaid-src)")) {
      if (pre.dataset.mathStyle) continue;
      const b = el("button", "copy", "Copy");
      b.onclick = async (e) => {
        e.stopPropagation();
        const text = pre.querySelector("code")?.innerText ?? pre.innerText;
        try { await navigator.clipboard.writeText(text); b.textContent = "Copied"; }
        catch { b.textContent = "Failed"; }
        setTimeout(() => (b.textContent = "Copy"), 1200);
      };
      pre.append(b);
    }
    renderMath();
    renderMermaid();
  }

  // ---------- lazy loaded renderers ----------
  const loaded = {};
  function loadScript(src) {
    return (loaded[src] ||= new Promise((res, rej) => {
      const s = el("script");
      s.src = src;
      s.onload = res;
      s.onerror = () => rej(new Error("failed to load " + src));
      document.head.append(s);
    }));
  }
  function loadCss(href) {
    if (loaded[href]) return;
    loaded[href] = true;
    const l = el("link");
    l.rel = "stylesheet";
    l.href = href;
    document.head.append(l);
  }

  async function renderMath() {
    const nodes = preview.querySelectorAll("[data-math-style]");
    if (!nodes.length) return;
    loadCss("vendor/katex/katex.min.css");
    try { await loadScript("vendor/katex/katex.min.js"); } catch { return; }
    for (const n of nodes) {
      const display = n.dataset.mathStyle === "display";
      const tex = n.textContent;
      const out = el(display ? "div" : "span", display ? "math-display" : "math-inline");
      if (n.dataset.sourcepos) out.dataset.sourcepos = n.dataset.sourcepos;
      try {
        window.katex.render(tex, out, { displayMode: display, throwOnError: false, output: "htmlAndMathml" });
        n.replaceWith(out);
      } catch { /* leave raw */ }
    }
  }

  let mermaidSeq = 0;
  async function renderMermaid(rerender = false) {
    let nodes = [...preview.querySelectorAll("pre.mermaid-src")];
    if (rerender) {
      for (const d of preview.querySelectorAll(".mermaid[data-done]")) {
        const pre = el("pre", "mermaid-src");
        pre.append(el("code", null, d.dataset.src));
        if (d.dataset.sourcepos) pre.dataset.sourcepos = d.dataset.sourcepos;
        d.replaceWith(pre);
        nodes.push(pre);
      }
    }
    if (!nodes.length) return;
    try { await loadScript("vendor/mermaid.min.js"); } catch { return; }
    const dark = document.documentElement.dataset.theme === "dark";
    window.mermaid.initialize({ startOnLoad: false, theme: dark ? "dark" : "default", securityLevel: "strict" });
    for (const pre of nodes) {
      if (!pre.isConnected) continue;
      const src = pre.textContent;
      const div = el("div", "mermaid");
      div.dataset.src = src;
      if (pre.dataset.sourcepos) div.dataset.sourcepos = pre.dataset.sourcepos;
      try {
        const { svg: out } = await window.mermaid.render(`mermaid-${++mermaidSeq}`, src);
        div.innerHTML = out;
        div.dataset.done = "1";
        pre.replaceWith(div);
      } catch (e) {
        pre.title = String(e.message || e);
        document.getElementById(`dmermaid-${mermaidSeq}`)?.remove();
      }
    }
  }

  // ---------- links ----------
  function scrollToAnchor(id) {
    const decoded = decodeURIComponent(id);
    const target = document.getElementById(decoded) || document.getElementById("user-content-" + decoded) ||
      preview.querySelector(`[name="${CSS.escape(decoded)}"]`);
    if (!target) return;
    target.scrollIntoView({ block: "start" });
    target.classList.remove("flash");
    void target.offsetWidth;
    target.classList.add("flash");
  }

  preview.addEventListener("click", async (e) => {
    const a = e.target.closest("a[href]");
    if (!a) return;
    const href = a.getAttribute("href");
    e.preventDefault();
    if (href.startsWith("#")) return scrollToAnchor(href.slice(1));
    if (EXTERNAL.test(href) && !/^file:/i.test(href)) return ipc("open_url", href);
    const [pathPart, hash] = href.split("#");
    if (!pathPart) return;
    const r = await api("resolve", { base: S.file || S.root, rel: pathPart });
    if (r.kind === "md") openFile(r.path, { anchor: hash || null });
    else if (r.kind === "dir") { setRoot(r.path); if (!S.showTree) toggleTree(); }
    else if (r.kind === "other") ipc("open_link_path", r.path);
    else showToast(`Not found: ${pathPart}`);
  });

  function showToast(msg) {
    const t = el("div", "state error", msg);
    t.style.cssText = "position:fixed;bottom:16px;left:50%;transform:translateX(-50%);margin:0;padding:8px 14px;border-radius:8px;background:var(--bg);border:1px solid var(--border);box-shadow:0 8px 24px rgba(0,0,0,.15);z-index:20";
    document.body.append(t);
    setTimeout(() => t.remove(), 2500);
  }

  // ---------- source pane + scroll sync ----------
  const sourcePane = $("#source-pane");

  async function loadSource(keepScroll = false) {
    if (!S.file || S.sourceLoadedFor === S.file) return;
    S.sourceLoadedFor = S.file;
    const scroll = sourcePane.scrollTop;
    try {
      const src = await api("source", { path: S.file });
      $("#source-code").innerHTML = src.html;
      let g = "";
      for (let i = 1; i <= src.lines; i++) g += i + "\n";
      $("#gutter").textContent = g;
    } catch (e) {
      $("#source-code").textContent = String(e.message || e);
      $("#gutter").textContent = "";
    }
    if (keepScroll) sourcePane.scrollTop = scroll;
    else syncFromPreview();
  }

  // Scroll sync maps preview offsets <-> source lines using the data-sourcepos
  // attributes comrak emits. Block offsets are measured once and cached, so
  // scrolling never forces a layout; the cache is dropped whenever the preview
  // changes size (images loading, diagrams rendering, window resize...).
  let anchors = null; // [{ top, height, start, end }] sorted by top
  let lineH = 0;
  const sourcePad = 12;
  const parsePos = (s) => {
    const m = /^(\d+):\d+-(\d+):\d+/.exec(s || "");
    return m ? [Number(m[1]), Number(m[2])] : null;
  };
  function getAnchors() {
    if (anchors) return anchors;
    anchors = [];
    for (const n of preview.children) {
      const pos = parsePos(n.dataset.sourcepos);
      if (pos) anchors.push({ top: n.offsetTop, height: n.offsetHeight, start: pos[0], end: pos[1] });
    }
    lineH = parseFloat(getComputedStyle($("#source")).lineHeight) || 20;
    return anchors;
  }
  const invalidateAnchors = () => (anchors = null);
  new ResizeObserver(invalidateAnchors).observe(preview);

  // Index of the last anchor whose key is <= value (binary search).
  function lastAtOrBefore(list, value, key) {
    let lo = 0, hi = list.length - 1, found = -1;
    while (lo <= hi) {
      const mid = (lo + hi) >> 1;
      if (list[mid][key] <= value) { found = mid; lo = mid + 1; } else hi = mid - 1;
    }
    return found;
  }

  let driver = "preview";
  previewPane.addEventListener("mouseenter", () => (driver = "preview"));
  previewPane.addEventListener("wheel", () => (driver = "preview"), { passive: true });
  sourcePane.addEventListener("mouseenter", () => (driver = "source"));
  sourcePane.addEventListener("wheel", () => (driver = "source"), { passive: true });

  function syncFromPreview() {
    if (!S.showSource) return;
    const y = previewPane.scrollTop;
    if (y <= 0) return void (sourcePane.scrollTop = 0);
    const list = getAnchors();
    const i = lastAtOrBefore(list, y, "top");
    if (i < 0) return;
    const a = list[i];
    const frac = a.height > 0 ? Math.min(1, (y - a.top) / a.height) : 0;
    const line = a.start + frac * (a.end - a.start + 1) - 1;
    sourcePane.scrollTop = sourcePad + line * lineH;
  }

  function syncFromSource() {
    const list = getAnchors();
    if (sourcePane.scrollTop <= 0) return void (previewPane.scrollTop = 0);
    const line = (sourcePane.scrollTop - sourcePad) / lineH + 1;
    const i = lastAtOrBefore(list, line, "start");
    if (i < 0) return;
    const a = list[i];
    const frac = Math.min(1, Math.max(0, (line - a.start) / (a.end - a.start + 1)));
    previewPane.scrollTop = a.top + frac * a.height;
  }

  let raf = 0;
  previewPane.addEventListener("scroll", () => {
    if (!S.showSource || driver !== "preview" || raf) return;
    raf = requestAnimationFrame(() => { raf = 0; syncFromPreview(); });
  }, { passive: true });
  sourcePane.addEventListener("scroll", () => {
    if (driver !== "source" || raf) return;
    raf = requestAnimationFrame(() => { raf = 0; syncFromSource(); });
  }, { passive: true });

  // ---------- find ----------
  const findbar = $("#findbar");
  const findInput = $("#find-input");
  function openFind() {
    findbar.hidden = false;
    findInput.focus();
    findInput.select();
  }
  function closeFind() {
    findbar.hidden = true;
    window.getSelection()?.removeAllRanges();
  }
  function find(backwards) {
    const q = findInput.value;
    if (!q) return;
    driver = "preview";
    // window.find is supported by WebKit and Chromium based webviews.
    const ok = window.find && window.find(q, false, backwards, true, false, false, false);
    findInput.style.borderColor = ok ? "" : "var(--caution)";
    findInput.focus();
  }
  findInput.addEventListener("keydown", (e) => {
    if (e.key === "Enter") { e.preventDefault(); find(e.shiftKey); }
    else if (e.key === "Escape") closeFind();
  });

  // ---------- live reload (called from Rust) ----------
  let fsTimer = 0;
  const pendingDirs = new Set();
  let pendingFile = false;
  function fsChanged(paths) {
    for (const p of paths) {
      // Only the current file's folder is watched, so a name match is enough.
      if (S.file && basename(p) === basename(S.file)) pendingFile = true;
      pendingDirs.add(dirname(p));
    }
    clearTimeout(fsTimer);
    fsTimer = setTimeout(async () => {
      if (pendingFile && S.file) {
        pendingFile = false;
        S.sourceLoadedFor = null;
        await openFile(S.file, { push: false, keepScroll: true });
      }
      for (const d of pendingDirs) refreshDir(d);
      pendingDirs.clear();
    }, 120);
  }

  async function openPath(path) {
    const r = await api("resolve", { base: S.root, rel: path });
    if (r.kind === "md") openFile(r.path);
    else if (r.kind === "dir") setRoot(r.path);
    else showToast("Not a Markdown file: " + basename(path));
  }

  window.mdview = { fsChanged, openPath };

  // ---------- keyboard ----------
  window.addEventListener("keydown", (e) => {
    const mod = e.ctrlKey || e.metaKey;
    const k = e.key.toLowerCase();
    const typing = e.target instanceof HTMLInputElement;
    if (mod && k === "b") { e.preventDefault(); toggleTree(); }
    else if (mod && k === "e") { e.preventDefault(); toggleSource(); }
    else if (mod && k === "f") { e.preventDefault(); openFind(); }
    else if (mod && k === "l") { e.preventDefault(); toggleTheme(); }
    else if (mod && k === "r") { e.preventDefault(); if (S.file) { S.sourceLoadedFor = null; openFile(S.file, { push: false, keepScroll: true }); } loadTree(); }
    else if (mod && (k === "w" || k === "q")) { e.preventDefault(); ipc("quit"); }
    else if (mod && (k === "=" || k === "+")) { e.preventDefault(); S.zoom = Math.min(2.5, +(S.zoom + 0.1).toFixed(2)); applyZoom(); }
    else if (mod && k === "-") { e.preventDefault(); S.zoom = Math.max(0.5, +(S.zoom - 0.1).toFixed(2)); applyZoom(); }
    else if (mod && k === "0") { e.preventDefault(); S.zoom = 1; applyZoom(); }
    else if (e.altKey && e.key === "ArrowLeft") { e.preventDefault(); go(-1); }
    else if (e.altKey && e.key === "ArrowRight") { e.preventDefault(); go(1); }
    else if (e.key === "Escape" && !findbar.hidden) closeFind();
    else if (e.key === "Backspace" && !typing && !mod) { e.preventDefault(); goUp(); }
  });
  window.addEventListener("mouseup", (e) => {
    if (e.button === 3) go(-1);
    if (e.button === 4) go(1);
  });

  function goUp() {
    const parent = dirname(S.root);
    if (parent && parent !== S.root) setRoot(parent);
  }

  // ---------- wire up ----------
  $("#btn-tree").innerHTML = ICON.sidebar;
  $("#btn-back").innerHTML = ICON.back;
  $("#btn-fwd").innerHTML = ICON.fwd;
  $("#btn-up").innerHTML = ICON.up;
  $("#btn-refresh").innerHTML = ICON.refresh;
  $("#btn-all").innerHTML = ICON.eye;
  $("#btn-find").innerHTML = ICON.search;
  $("#btn-source .ico").innerHTML = ICON.code;
  $("#find-prev").innerHTML = ICON.chevUp;
  $("#find-next").innerHTML = ICON.chevDown;
  $("#find-close").innerHTML = ICON.close;

  $("#btn-tree").onclick = toggleTree;
  $("#btn-source").onclick = toggleSource;
  $("#btn-theme").onclick = toggleTheme;
  $("#btn-back").onclick = () => go(-1);
  $("#btn-fwd").onclick = () => go(1);
  $("#btn-up").onclick = goUp;
  $("#btn-refresh").onclick = loadTree;
  $("#btn-find").onclick = () => (findbar.hidden ? openFind() : closeFind());
  $("#find-next").onclick = () => find(false);
  $("#find-prev").onclick = () => find(true);
  $("#find-close").onclick = closeFind;
  $("#btn-all").onclick = () => {
    S.showAll = !S.showAll;
    store.set("showAll", S.showAll);
    applyLayout();
    loadTree();
  };

  async function init() {
    applyTheme();
    applyZoom();
    applyLayout();
    updateNav();
    let info;
    try {
      info = await api("init", {});
    } catch (e) {
      showState("Failed to start", String(e.message || e), true);
      return;
    }
    S.sep = info.sep;
    S.root = info.root;
    renderCrumbs();
    ipc("watch", info.root);
    if (info.file) {
      await openFile(info.file);
      loadTree();
    } else {
      await loadTree();
      if (info.error) showState("File not found", info.error.replace(/^File not found: /, ""), true);
      else showState("No file open", "Pick a Markdown file from the tree on the left.");
    }
  }
  init();
})();
