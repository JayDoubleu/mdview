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
    pencil: svg('<path d="M11.2 2.3a1.4 1.4 0 0 1 2 0l.5.5a1.4 1.4 0 0 1 0 2L6 12.5l-3 .8.8-3z"/><path d="M10 3.5l2.5 2.5"/>'),
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
    editing: false,
    dirty: false,
    savedText: "",
    docName: "",
    previewIsBuffer: false, // preview shows the editor buffer, not the file
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
    const d = i <= 0 ? p.slice(0, i + 1) || p : p.slice(0, i);
    return /^[A-Za-z]:$/.test(d) ? d + "\\" : d; // "C:\\a.md" -> "C:\\"
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
    document.body.classList.toggle("no-source", !S.showSource && !S.editing);
    $("#btn-tree").classList.toggle("on", S.showTree);
    $("#btn-source").classList.toggle("on", S.showSource);
    $("#btn-all").classList.toggle("on", S.showAll);
    document.documentElement.style.setProperty("--tree-w", S.treeW + "px");
    if (S.sourceW) document.documentElement.style.setProperty("--source-w", S.sourceW + "px");
    if (S.showSource && !S.editing) loadSource();
  }

  function toggleTree() { S.showTree = !S.showTree; store.set("showTree", S.showTree); applyLayout(); }
  function toggleSource() {
    if (S.editing) return showToast("The editor replaces the source pane while editing");
    S.showSource = !S.showSource; store.set("showSource", S.showSource); applyLayout(); syncFromPreview(); }

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
    syncWatches();
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
    if (data.entries.some((e) => e.dir)) loadCounts(ul, data.path);
  }

  // Badge each folder with how many Markdown files it holds (including subfolders).
  // Counted in the background, so the tree shows straight away.
  async function loadCounts(ul, dir) {
    let data;
    try { data = await api("counts", { path: dir }); } catch { return; }
    for (const li of ul.querySelectorAll(":scope > li.dir")) {
      const c = data.counts[li.dataset.path];
      const badge = li.querySelector(":scope > .row > .count");
      if (!c || !badge) continue;
      badge.textContent = c.n + (c.partial ? "+" : "");
      badge.classList.toggle("zero", c.n === 0 && !c.partial);
      badge.title = c.partial
        ? `At least ${c.n} Markdown files (stopped counting: very large folder)`
        : `${c.n} Markdown file${c.n === 1 ? "" : "s"} in this folder and its subfolders`;
      badge.hidden = false;
    }
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
    if (ent.dir) {
      const count = el("span", "count");
      count.hidden = true;
      row.append(count);
    }
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
      syncWatches();
      return;
    }
    li.classList.add("open");
    if (!sub) {
      sub = el("ul");
      li.append(sub);
      await fillDir(sub, li.dataset.path, depth + 1);
    }
    sub.hidden = false;
    syncWatches();
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

  // Watch every folder the tree shows, plus the open file's folder, so
  // changes made by other programs appear without a manual refresh.
  let watchTimer = 0;
  function syncWatches() {
    clearTimeout(watchTimer);
    watchTimer = setTimeout(() => {
      const dirs = new Set();
      if (S.root) dirs.add(S.root);
      if (S.file) dirs.add(dirname(S.file));
      for (const li of tree.querySelectorAll("li.dir.open")) {
        if (!li.parentElement.closest("li.dir:not(.open)")) dirs.add(li.dataset.path);
      }
      ipc("watch", JSON.stringify([...dirs]));
    }, 50);
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

  // Folder badges above a changed folder count its files too.
  function refreshCounts(dir) {
    for (const ul of [tree, ...tree.querySelectorAll("ul")]) {
      const p = ul.dataset.path;
      if (p && !samePath(p, dir) && isUnder(dir, p)) loadCounts(ul, p);
    }
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
    if (!(await confirmLeave())) return false;
    document.querySelector(".undo-toast")?.remove();
    cancelRender();
    let doc;
    const busy = setBusy();
    try {
      doc = await api("render", { path });
    } catch (e) {
      busy();
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
    S.previewIsBuffer = false;
    if (!doc.html.trim() && !S.editing) showState("This file is empty", "Click Edit to start writing.");
    enhance();
    busy();
    setDocPath(doc.path);
    S.docName = doc.name;
    updateDirtyUI();
    syncWatches();

    if (!isUnder(doc.path, S.root) && changed) setRoot(doc.dir);
    else if (changed) revealInTree(doc.path);

    if (keepScroll) previewPane.scrollTop = scroll;
    else if (anchor) scrollToAnchor(anchor);
    else previewPane.scrollTop = 0;

    S.sourceLoadedFor = null;
    if (S.editing) await loadEditor(keepScroll);
    else if (S.showSource) await loadSource(keepScroll);
    return true;
  }

  // Shows a small "Rendering…" note when loading takes more than a moment.
  // Returns the function that clears it.
  function setBusy() {
    const t = setTimeout(() => document.body.classList.add("busy"), 300);
    return () => { clearTimeout(t); document.body.classList.remove("busy"); };
  }

  function updateNav() {
    $("#btn-back").disabled = S.hIndex <= 0;
    $("#btn-fwd").disabled = S.hIndex >= S.history.length - 1;
  }
  async function go(delta) {
    const i = S.hIndex + delta;
    if (i < 0 || i >= S.history.length) return;
    const prev = S.hIndex;
    S.hIndex = i;
    if (!(await openFile(S.history[i], { push: false }))) {
      S.hIndex = prev;
      updateNav();
    }
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
    // Annotation cards: a Resolve button removes the note from the file.
    for (const card of preview.querySelectorAll("aside.annotation[data-sourcepos]")) {
      const bar = el("div", "annotation-actions");
      const b = el("button", null, "Resolve");
      b.title = "Remove this note from the file (Save to keep the change)";
      b.onclick = (e) => { e.stopPropagation(); resolveAnnotation(card); };
      bar.append(b);
      card.append(bar);
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
        document.getElementById(`dmermaid-${mermaidSeq}`)?.remove();
        const msg = String(e.message || e).split("\n").find((l) => l.trim()) || "invalid diagram";
        const note = el("div", "render-error", `Mermaid diagram error: ${msg}`);
        pre.after(note);
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

  function showToast(msg, isError = true) {
    const t = el("div", "state" + (isError ? " error" : ""), msg);
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
    const busy = setBusy();
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
    busy();
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
    lineH = parseFloat(getComputedStyle(S.editing ? editor : $("#source")).lineHeight) || 20;
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
    if (!S.showSource && !S.editing) return;
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
    if (!(S.showSource || S.editing) || driver !== "preview" || raf) return;
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
      if (S.file && basename(p) === basename(S.file) && samePath(dirname(p), dirname(S.file))) pendingFile = true;
      pendingDirs.add(dirname(p));
    }
    clearTimeout(fsTimer);
    fsTimer = setTimeout(async () => {
      if (pendingFile && S.file) {
        pendingFile = false;
        if (S.editing) await diskChangedWhileEditing();
        else {
          S.sourceLoadedFor = null;
          await openFile(S.file, { push: false, keepScroll: true });
        }
      }
      for (const d of pendingDirs) refreshDir(d).then(() => refreshCounts(d));
      pendingDirs.clear();
      syncWatches();
    }, 120);
  }

  async function openPath(path) {
    const r = await api("resolve", { base: S.root, rel: path });
    if (r.kind === "md") openFile(r.path);
    else if (r.kind === "dir") setRoot(r.path);
    else showToast("Not a Markdown file: " + basename(path));
  }

  // ---------- edit mode ----------
  // The editor works on a buffer. The preview renders that buffer live, but
  // nothing touches the file until Save; the toolbar pill, the banner over the
  // preview and the dot in the title show when the buffer differs from disk.
  const editor = $("#editor");
  let rpcSeq = 0;
  const rpcPending = new Map();

  // Requests answered by the app through `mdview.reply` (large text goes over IPC).
  function rpc(cmd, arg, extra = {}) {
    const id = ++rpcSeq;
    return new Promise((resolve) => {
      rpcPending.set(id, resolve);
      try { window.ipc.postMessage(JSON.stringify({ cmd, arg, id, ...extra })); }
      catch { rpcPending.delete(id); resolve(null); }
    });
  }
  function reply({ id, data }) {
    const resolve = rpcPending.get(id);
    rpcPending.delete(id);
    if (resolve) resolve(data);
  }

  async function readRaw(path) {
    const r = await fetch(`api/raw?${new URLSearchParams({ path })}`);
    if (!r.ok) throw new Error(await r.text());
    S.encoding = r.headers.get("X-Mdview-Encoding") || "UTF-8";
    S.lossy = r.headers.get("X-Mdview-Lossy") === "1";
    // Textareas use \n; compare and edit in that form.
    return (await r.text()).replace(/\r\n?/g, "\n");
  }

  // The textarea grows to fit its text so the pane scrolls it together with
  // the line numbers and the current-line band.
  let lineCount = 0;
  function fitEditor() {
    editor.style.height = "auto";
    editor.style.height = editor.scrollHeight + "px";
    editor.style.width = "100%";
    if (editor.scrollWidth > editor.clientWidth) editor.style.width = editor.scrollWidth + "px";
    const n = editor.value.split("\n").length;
    if (n !== lineCount) {
      lineCount = n;
      let g = "";
      for (let i = 1; i <= n; i++) g += `<span>${i}</span>\n`;
      $("#editor-numbers").innerHTML = g;
      curRange = "";
    }
    updateCursor();
  }

  // ---- cursor / selection: a band in the editor, a soft tint in the preview ----
  // Works for a caret (one line) and for multi-line selections, and also for
  // selections in the read-only source pane.
  let curRange = "";
  let cursorRaf = 0;
  const BLOCKS = "p,li,h1,h2,h3,h4,h5,h6,pre,blockquote,tr,dt,dd,hr,table,.markdown-alert,.math-display,.mermaid,.front-matter,.annotation";

  function lineAt(text, pos) {
    let n = 1;
    for (let i = text.indexOf("\n"); i !== -1 && i < pos; i = text.indexOf("\n", i + 1)) n++;
    return n;
  }

  // First and last selected line; a selection ending at the start of a line
  // (e.g. after selecting whole lines) doesn't include that line.
  function editorRange() {
    const text = editor.value;
    const { selectionStart: s0, selectionEnd: s1 } = editor;
    const a = lineAt(text, s0);
    let b = s1 > s0 ? lineAt(text, s1) : a;
    if (b > a && text[s1 - 1] === "\n") b--;
    return [a, b];
  }

  function updateCursor(force = false) {
    if (!S.editing) return;
    const [a, b] = editorRange();
    const key = a + ":" + b;
    if (key === curRange && !force) return;
    curRange = key;
    const lh = parseFloat(getComputedStyle(editor).lineHeight) || 20;
    for (const band of [$("#editor-line"), $("#gutter-line")]) {
      band.style.top = 12 + (a - 1) * lh + "px";
      band.style.height = (b - a + 1) * lh + "px";
    }
    const nums = $("#editor-numbers");
    for (const n of nums.querySelectorAll(".cur")) n.classList.remove("cur");
    for (let i = a; i <= b; i++) nums.children[i - 1]?.classList.add("cur");
    markPreview(a, b, document.activeElement === editor);
  }
  const scheduleCursor = () => {
    if (!cursorRaf) cursorRaf = requestAnimationFrame(() => { cursorRaf = 0; updateCursor(); });
  };

  function clearCursorBlock() {
    for (const n of preview.querySelectorAll(".cursor-block")) n.classList.remove("cursor-block");
  }

  // Tint the innermost blocks whose source lines overlap lines a..b.
  function markPreview(a, b, reveal) {
    clearCursorBlock();
    const hits = [];
    for (const n of preview.querySelectorAll(BLOCKS)) {
      const pos = parsePos(n.dataset.sourcepos);
      if (pos && pos[0] <= b && pos[1] >= a) hits.push(n);
    }
    // Keep only the innermost matches (a paragraph rather than its list item).
    const leaves = hits.filter((n) => !hits.some((m) => m !== n && n.contains(m)));
    for (const n of leaves) {
      n.classList.add("cursor-block");
      // Open the front matter panel when the cursor is inside it.
      if (n.matches("details.front-matter")) n.open = true;
    }
    if (!reveal || !leaves.length) return;
    const r = leaves[0].getBoundingClientRect();
    const v = previewPane.getBoundingClientRect();
    if (r.bottom < v.top + 30 || r.top > v.bottom - 30) {
      previewPane.scrollTop += r.top - v.top - previewPane.clientHeight / 3;
    }
  }

  // Read-only source pane: selecting (or clicking) lines tints them in the preview.
  let sourceMarked = false;
  let sourceRaf = 0;
  function sourceLine(node) {
    const code = $("#source-code");
    const el = (node && node.nodeType === 1 ? node : node?.parentElement)?.closest?.(".line");
    return el && el.parentElement === code ? Array.prototype.indexOf.call(code.children, el) + 1 : 0;
  }
  function updateSourceSelection() {
    const sel = window.getSelection();
    const range = sel && sel.rangeCount ? sel.getRangeAt(0) : null;
    const a = range ? sourceLine(range.startContainer) : 0;
    let b = range ? sourceLine(range.endContainer) : 0;
    if (!a || !b) {
      if (sourceMarked) { clearCursorBlock(); sourceMarked = false; }
      return;
    }
    if (b > a && range.endOffset === 0) b--;
    markPreview(a, b, true);
    sourceMarked = true;
  }

  // ---- preview → source: selecting text in the preview selects it in the
  // editor (edit mode) or highlights it in the source pane (view mode) ----
  const FULL_POS = /^(\d+):(\d+)-(\d+):(\d+)/;

  // Character index of 1-based line `line`, byte column `col` (comrak counts
  // UTF-8 bytes) in `text`.
  function textOffset(text, line, col) {
    let i = 0;
    for (let n = 1; n < line && i !== -1; n++) i = text.indexOf("\n", i) + 1 || -1;
    if (i < 0) return text.length;
    let bytes = 0;
    while (i < text.length && text[i] !== "\n" && bytes < col - 1) {
      const c = text.codePointAt(i);
      bytes += c < 0x80 ? 1 : c < 0x800 ? 2 : c < 0x10000 ? 3 : 4;
      i += c > 0xffff ? 2 : 1;
    }
    return i;
  }

  // Walks the rendered text through the source: every visible character is
  // looked for in order, skipping Markdown syntax (`**`, `[..](url)`, `&amp;`)
  // in between. Whitespace is ignored since rendering reflows it.
  function followText(rendered, text, from, to) {
    let k = from;
    for (const ch of rendered) {
      if (/\s/.test(ch)) continue;
      const j = text.indexOf(ch, k);
      if (j === -1 || j >= to) return to;
      k = j + ch.length;
    }
    if (/\s$/.test(rendered)) while (k < to && /[ \t]/.test(text[k])) k++;
    return k;
  }

  // Source offset of a DOM point inside the preview, or -1.
  function previewPointToOffset(text, node, offset, isEnd) {
    // A point between blocks: use the start of the next / end of the previous.
    if (node.nodeType === 1 && node.childNodes.length && !node.closest("[data-sourcepos]")) {
      const kids = node.childNodes;
      let n = isEnd ? kids[offset - 1] : kids[offset];
      while (n && !(n.nodeType === 1 && n.matches("[data-sourcepos]"))) {
        n = isEnd ? n.previousSibling : n.nextSibling;
      }
      if (!n) return -1;
      const m = FULL_POS.exec(n.dataset.sourcepos);
      return isEnd ? Math.min(text.length, textOffset(text, +m[3], +m[4] + 1)) : textOffset(text, +m[1], +m[2]);
    }
    const start = (node.nodeType === 1 ? node : node.parentElement)?.closest("[data-sourcepos]");
    if (!start || !preview.contains(start)) return -1;
    const m = FULL_POS.exec(start.dataset.sourcepos);
    if (!m) return -1;
    let from = textOffset(text, +m[1], +m[2]);
    const to = Math.min(text.length, textOffset(text, +m[3], +m[4] + 1));
    let root = start;
    // Code blocks and front matter: skip the fence line, ignore the summary.
    const pre = (node.nodeType === 1 ? node : node.parentElement).closest("pre");
    if (pre && start.contains(pre)) {
      root = pre;
      if (/^(```|~~~|---|\+\+\+)/.test(text.slice(from, from + 3))) {
        from = text.indexOf("\n", from) + 1 || to;
      }
    } else if (start.matches(".front-matter")) {
      return from;
    }
    const r = document.createRange();
    r.setStart(root, 0);
    try { r.setEnd(node, offset); } catch { return from; }
    const before = r.toString();
    const k = followText(before, text, from, to);
    // At the very start of a block, skip its syntax (`- `, `# `, `> `, `| `)
    // and start at the first visible character.
    if (!isEnd && !before.trim()) {
      const after = document.createRange();
      after.selectNodeContents(root);
      try { after.setStart(node, offset); } catch { return k; }
      const c = after.toString().trimStart()[0];
      const j = c ? text.indexOf(c, k) : -1;
      if (j !== -1 && j < to) return j;
    }
    return k;
  }

  function previewSelectionRange(text, range) {
    if (!range) {
      const sel = window.getSelection();
      if (!sel || !sel.rangeCount) return null;
      range = sel.getRangeAt(0);
    }
    if (!preview.contains(range.commonAncestorContainer)) return null;
    // A re-render resets the selection to the top; that's not a user pick.
    if (range.collapsed && range.startContainer === preview) return null;
    let s = previewPointToOffset(text, range.startContainer, range.startOffset, false);
    let e = range.collapsed ? s : previewPointToOffset(text, range.endContainer, range.endOffset, true);
    if (s < 0 && e < 0) return null;
    if (s < 0) s = e;
    if (e < 0) e = s;
    return s <= e ? [s, e] : [e, s];
  }

  // An unfocused textarea doesn't paint its selection, so mirror it.
  const editorPick = $("#editor-pick");
  function showEditorPick(s, e) {
    if (s === e) return hideEditorPick();
    const v = editor.value;
    editorPick.replaceChildren(v.slice(0, s), el("mark", "", v.slice(s, e)));
    editorPick.hidden = false;
  }
  function hideEditorPick() {
    if (!editorPick.hidden) editorPick.hidden = true;
  }

  let sourceHl = false;
  function clearSourceHighlight() {
    hideEditorPick();
    if (!sourceHl) return;
    sourceHl = false;
    for (const n of document.querySelectorAll("#source-code .line.picked")) n.classList.remove("picked");
    if (window.CSS?.highlights) CSS.highlights.delete("mdview-picked");
  }

  // DOM point for a character index in the read-only source pane.
  function sourcePoint(lines, line, col) {
    const walker = document.createTreeWalker(lines[line - 1], NodeFilter.SHOW_TEXT);
    let last = null;
    for (let t = walker.nextNode(); t; t = walker.nextNode()) {
      if (col <= t.length) return [t, col];
      col -= t.length;
      last = t;
    }
    return last ? [last, last.length] : [lines[line - 1], 0];
  }

  function revealSourceLine(line) {
    const lh = parseFloat(getComputedStyle(S.editing ? editor : $("#source")).lineHeight) || 20;
    const y = sourcePad + (line - 1) * lh;
    if (y < sourcePane.scrollTop + lh || y > sourcePane.scrollTop + sourcePane.clientHeight - 2 * lh) {
      sourcePane.scrollTop = Math.max(0, y - sourcePane.clientHeight / 3);
    }
  }

  function syncPreviewSelection() {
    if (S.editing) {
      const r = previewSelectionRange(editor.value);
      if (!r) return;
      if (editor.selectionStart === r[0] && editor.selectionEnd === r[1]) return;
      editor.setSelectionRange(r[0], r[1]);
      showEditorPick(r[0], r[1]);
      updateCursor(true);
      revealSourceLine(lineAt(editor.value, r[0]));
      return;
    }
    if (!S.showSource) return;
    const code = $("#source-code");
    const lines = code.children;
    const r = previewSelectionRange(code.textContent);
    const sel = window.getSelection();
    clearSourceHighlight();
    if (!r || !lines.length || sel.isCollapsed) return;
    const text = code.textContent;
    const a = lineAt(text, r[0]);
    let b = lineAt(text, r[1]);
    if (b > a && text[r[1] - 1] === "\n") b--;
    for (let i = a; i <= b; i++) lines[i - 1]?.classList.add("picked");
    sourceHl = true;
    if (window.CSS?.highlights && window.Highlight) {
      const col = (pos) => pos - (text.lastIndexOf("\n", pos - 1) + 1);
      const hl = document.createRange();
      hl.setStart(...sourcePoint(lines, a, col(r[0])));
      hl.setEnd(...sourcePoint(lines, lineAt(text, r[1]), col(r[1])));
      CSS.highlights.set("mdview-picked", new Highlight(hl));
    }
    revealSourceLine(a);
  }

  async function loadEditor(keepScroll = false) {
    const scroll = sourcePane.scrollTop;
    const path = S.file;
    cancelRender();
    // Until the new text is in, the buffer belongs to no file: no typing, no saving.
    S.editPath = null;
    editor.readOnly = true;
    try {
      S.savedText = await readRaw(path);
    } catch (e) {
      editor.readOnly = false;
      showToast(String(e.message || e));
      return leaveEdit(true);
    }
    if (!samePath(path, S.file)) return; // another file was opened meanwhile
    editor.readOnly = false;
    S.editPath = path;
    editor.value = S.savedText;
    fitEditor();
    updateCursor(true);
    setDirty(false);
    if (keepScroll) sourcePane.scrollTop = scroll;
    else syncFromPreview();
  }

  async function enterEdit() {
    if (!S.file) return showToast("Open a Markdown file to edit it");
    S.editing = true;
    document.body.classList.add("editing");
    $("#editor-wrap").hidden = false;
    $("#btn-discard").hidden = false;
    $("#btn-edit").classList.add("on");
    $("#btn-edit .label").textContent = "Editing";
    $("#btn-edit").title = "Lock editing";
    applyLayout();
    await loadEditor();
    if (S.lossy) {
      showToast(`This file has bytes that aren't valid ${S.encoding}. mdview won't save it, to avoid damaging them.`);
    } else if (S.encoding && S.encoding !== "UTF-8") {
      showToast(`This file is ${S.encoding}. Saving keeps that encoding.`, false);
    }
    editor.setSelectionRange(0, 0);
    editor.focus({ preventScroll: true });
    driver = "preview";
    syncFromPreview();
  }

  async function leaveEdit(force = false) {
    if (!force && !(await confirmLeave())) return;
    const restore = S.previewIsBuffer;
    S.previewIsBuffer = false;
    S.editing = false;
    S.editPath = null;
    cancelRender();
    document.querySelector(".undo-toast")?.remove();
    setDirty(false);
    $("#editor-wrap").hidden = true;
    $("#btn-discard").hidden = true;
    clearCursorBlock();
    hideEditorPick();
    editor.value = "";
    document.body.classList.remove("editing");
    $("#btn-edit").classList.remove("on");
    $("#btn-edit .label").textContent = "Edit";
    $("#btn-edit").title = "Unlock editing";
    applyLayout();
    // Show the file as it is on disk again.
    if (restore && S.file) await openFile(S.file, { push: false, keepScroll: true });
  }

  function setDirty(dirty) {
    if (S.dirty === dirty) return;
    S.dirty = dirty;
    updateDirtyUI();
    ipc("dirty", dirty ? "1" : "0");
  }

  function updateDirtyUI() {
    document.body.classList.toggle("dirty", S.dirty);
    $("#unsaved").hidden = !S.dirty;
    $("#btn-save").hidden = !S.dirty;
    $("#btn-discard").disabled = !S.dirty;
    $("#draft-banner").hidden = !S.dirty;
    if (S.docName) ipc("title", `${S.dirty ? "● " : ""}${S.docName} — mdview`);
  }

  // While the preview lags behind the buffer its source positions are stale,
  // so the actions that use them (Resolve) are hidden until it catches up.
  let renderTimer = 0;
  let renderSeq = 0;
  function onEdit() {
    fitEditor();
    setDirty(editor.value !== S.savedText);
    clearTimeout(renderTimer);
    S.renderPending = true;
    preview.classList.add("stale");
    renderTimer = setTimeout(renderBuffer, 120);
  }

  function cancelRender() {
    clearTimeout(renderTimer);
    renderSeq++;
    S.renderPending = false;
    preview.classList.remove("stale");
  }

  // Brings the preview up to date with the buffer now.
  async function flushRender() {
    if (!S.editing || !S.renderPending) return;
    clearTimeout(renderTimer);
    await renderBuffer();
  }

  async function renderBuffer() {
    const seq = ++renderSeq;
    const file = S.file;
    const res = await rpc("render_text", editor.value);
    if (!res || seq !== renderSeq || !S.editing || !samePath(file, S.file)) return;
    S.renderPending = false;
    preview.classList.remove("stale");
    const scroll = previewPane.scrollTop;
    preview.innerHTML = res.html;
    enhance();
    previewPane.scrollTop = scroll;
    S.previewIsBuffer = true;
    revealCaret();
    curRange = "";
    updateCursor();
  }

  // Keep the block being typed into visible in the preview.
  function revealCaret() {
    if (document.activeElement !== editor) return;
    const line = editor.value.slice(0, editor.selectionStart).split("\n").length;
    const list = getAnchors();
    const i = lastAtOrBefore(list, line, "start");
    if (i < 0) return;
    const a = list[i];
    const view = previewPane.clientHeight;
    const top = previewPane.scrollTop;
    if (a.top < top + 40 || a.top + Math.min(a.height, view / 2) > top + view) {
      previewPane.scrollTop = Math.max(0, a.top - view / 3);
    }
  }

  async function save() {
    if (!S.editing || !S.editPath) return false;
    const text = editor.value;
    const res = await rpc("save", text, { path: S.editPath });
    if (!res || !res.ok) {
      showToast(`Couldn't save: ${(res && res.error) || "unknown error"}`);
      return false;
    }
    S.savedText = text;
    setDirty(editor.value !== S.savedText);
    showToast(res.note || "Saved", false);
    return true;
  }

  // Resolves true when it's fine to drop the buffer (saved, discarded or clean).
  async function confirmLeave() {
    if (!S.dirty) return true;
    const choice = await askUnsaved();
    if (choice === "save") return save();
    if (choice === "discard") {
      editor.value = S.savedText;
      setDirty(false);
      return true;
    }
    editor.focus();
    return false;
  }

  function askUnsaved() {
    const modal = $("#modal");
    $("#modal-text").textContent =
      `Your changes to ${S.docName || "this file"} haven't been saved. If you continue without saving, they will be lost.`;
    modal.hidden = false;
    $("#modal-cancel").focus();
    return new Promise((resolve) => {
      const done = (choice) => {
        modal.hidden = true;
        modal.onkeydown = null;
        resolve(choice);
      };
      $("#modal-save").onclick = () => done("save");
      $("#modal-discard").onclick = () => done("discard");
      $("#modal-cancel").onclick = () => done("cancel");
      modal.onclick = (e) => { if (e.target === modal) done("cancel"); };
      modal.onkeydown = (e) => {
        if (e.key === "Escape") { e.preventDefault(); done("cancel"); }
        e.stopPropagation();
      };
    });
  }

  async function diskChangedWhileEditing() {
    let disk;
    try { disk = await readRaw(S.file); } catch { return; }
    if (disk === S.savedText) return; // our own save, or no real change
    if (!S.dirty) {
      await loadEditor(true);
      renderBuffer();
    } else {
      S.savedText = disk;
      setDirty(editor.value !== disk);
      showToast(`${S.docName} changed on disk. Saving will overwrite those changes.`);
    }
  }

  // Revert the buffer to the file on disk. Done as an edit so Ctrl+Z brings it back.
  function discardChanges() {
    if (!S.editing || !S.dirty) return;
    const discarded = editor.value;
    editor.focus({ preventScroll: true });
    editor.select();
    if (!document.execCommand("insertText", false, S.savedText)) editor.value = S.savedText;
    editor.setSelectionRange(0, 0);
    onEdit();
    const file = S.file;
    showUndoToast("Changes discarded", () => {
      if (!S.editing || !samePath(file, S.editPath)) return;
      editor.value = discarded;
      onEdit();
      editor.focus({ preventScroll: true });
    });
  }

  function showUndoToast(msg, onUndo) {
    document.querySelector(".undo-toast")?.remove();
    const t = el("div", "undo-toast");
    const b = el("button", null, "Undo");
    b.onclick = () => { t.remove(); onUndo(); };
    t.append(el("span", null, msg), b);
    document.body.append(t);
    setTimeout(() => t.remove(), 8000);
  }

  // ---------- annotations ----------
  // A note on a selection, stored in the file as an HTML comment just above
  // the block it belongs to (see src/annotations.rs for the format).

  // What is selected right now, in source terms where that's known already.
  function annotationTarget() {
    if (S.editing && editor.selectionStart < editor.selectionEnd) {
      const sel = window.getSelection();
      const inPreview = sel && sel.rangeCount && !sel.isCollapsed && preview.contains(sel.getRangeAt(0).commonAncestorContainer);
      if (document.activeElement === editor || inPreview) {
        return { s: editor.selectionStart, e: editor.selectionEnd, text: editor.value };
      }
    }
    const sel = window.getSelection();
    if (!sel || !sel.rangeCount || sel.isCollapsed) return null;
    const r = sel.getRangeAt(0);
    if (preview.contains(r.commonAncestorContainer)) return { range: r.cloneRange() };
    const code = $("#source-code");
    if (!S.editing && code.contains(r.commonAncestorContainer)) {
      const before = document.createRange();
      before.setStart(code, 0);
      before.setEnd(r.startContainer, r.startOffset);
      const start = before.toString().length;
      return { s: start, e: start + r.toString().length, text: code.textContent };
    }
    return null;
  }

  async function annotate(t) {
    if (!S.file) return;
    if (t.range) {
      // Map the preview selection onto the text the preview was made from:
      // the buffer while editing, the file otherwise.
      let raw;
      if (S.editing) {
        if (S.renderPending) return showToast("The preview is still updating. Try again in a moment.");
        raw = editor.value;
      } else {
        try { raw = await readRaw(S.file); } catch (e) { return showToast(String(e.message || e)); }
      }
      const r = previewSelectionRange(raw, t.range);
      if (!r) return showToast("Couldn't find that selection in the source");
      t = { s: r[0], e: r[1], text: raw };
    }
    // Whole words make a better anchor than "l ship the ne".
    const word = /[\p{L}\p{N}_]/u;
    while (t.s > 0 && word.test(t.text[t.s - 1]) && word.test(t.text[t.s])) t.s--;
    while (t.e < t.text.length && word.test(t.text[t.e]) && word.test(t.text[t.e - 1])) t.e++;
    const quote = t.text.slice(t.s, t.e).replace(/\s+$/, "");
    if (!quote.trim()) return showToast("Select some text to annotate");
    const note = await askNote(quote);
    if (!note) return;
    if (!S.editing) {
      await enterEdit();
      if (!S.editing) return;
    }
    // Block positions come from the preview, so it must match the buffer.
    await flushRender();
    // The file may differ from what was selected in (e.g. line endings): find
    // the quote again, nearest to where it was.
    const text = editor.value;
    let at = t.s;
    if (text.slice(at, at + quote.length) !== quote) {
      let best = -1;
      for (let i = text.indexOf(quote); i !== -1; i = text.indexOf(quote, i + 1)) {
        if (best < 0 || Math.abs(i - t.s) < Math.abs(best - t.s)) best = i;
      }
      if (best >= 0) at = best;
    }
    insertAnnotation(Math.min(at, text.length), quote, note);
  }

  function annotationText(quote, note) {
    // Comments end at --> (and at --!> in browsers); break both up.
    const safe = (x) => x.replace(/--(!?)>/g, "-- $1>");
    let q = quote;
    if (q.length > 400) q = q.slice(0, 220) + " … " + q.slice(-150);
    q = safe(q);
    const selected = q.includes("\n")
      ? "Selected:\n" + q.split("\n").map((l) => "> " + l).join("\n")
      : `Selected: "${q}"`;
    return `<!-- ANNOTATION\n${selected}\nNote: ${safe(note.trim())}\n-->`;
  }

  // Puts the note above the top-level block holding offset `at`, so it never
  // lands inside a list, table or code block.
  function insertAnnotation(at, quote, note) {
    const text = editor.value;
    const line = lineAt(text, at);
    let target = line;
    for (const n of preview.children) {
      const pos = parsePos(n.dataset.sourcepos);
      if (!pos) continue;
      if (pos[0] > line) break;
      if (line <= pos[1]) target = n.matches(".front-matter") ? pos[1] + 1 : pos[0];
    }
    const pos = textOffset(text, target, 1);
    const prevStart = text.lastIndexOf("\n", pos - 2) + 1;
    let insert = annotationText(quote, note);
    if (pos > 0 && text[pos - 1] !== "\n") insert = "\n\n" + insert;
    else if (pos > 0 && text.slice(prevStart, pos).trim()) insert = "\n" + insert;
    insert += pos < text.length ? "\n\n" : "\n";
    editor.focus({ preventScroll: true });
    editor.setSelectionRange(pos, pos);
    if (!document.execCommand("insertText", false, insert)) {
      editor.setRangeText(insert, pos, pos, "end");
      onEdit();
    }
    revealSourceLine(lineAt(editor.value, pos));
    showToast("Note added. Save to keep it in the file.", false);
  }

  async function resolveAnnotation(card) {
    const m = FULL_POS.exec(card.dataset.sourcepos || "");
    if (!m || preview.classList.contains("stale")) return;
    const [line, col, last] = [+m[1], +m[2], +m[3]];
    if (!S.editing) {
      await enterEdit();
      if (!S.editing) return;
    }
    // Delete only what is really that note: the marker where the card says,
    // closed on the card's last line. Anything else means the card is stale
    // (or isn't a real note), and nothing is touched.
    const text = editor.value;
    const at = textOffset(text, line, col);
    const close = text.indexOf("-->", at);
    if (!text.startsWith("<!-- ANNOTATION", at) || close === -1 || lineAt(text, close) !== last) {
      return showToast("Couldn't find this note in the file");
    }
    const lineStart = textOffset(text, line, 1);
    const a = /^[\s>]*$/.test(text.slice(lineStart, at)) ? lineStart : at;
    let b = close + 3;
    const eol = (i) => { const n = text.indexOf("\n", i); return n === -1 ? text.length : n + 1; };
    if (!text.slice(b, eol(b)).trim()) {
      b = eol(b);
      // Take the blank line after it too.
      if (b < text.length && !text.slice(b, eol(b)).trim()) b = eol(b);
    }
    editor.focus({ preventScroll: true });
    editor.setSelectionRange(a, b);
    if (!document.execCommand("delete")) {
      editor.setRangeText("", a, b, "end");
      onEdit();
    }
  }

  function askNote(quote) {
    const modal = $("#annotate");
    const input = $("#annotate-note");
    const ok = $("#annotate-ok");
    $("#annotate-quote").textContent = quote.length > 600 ? quote.slice(0, 600) + " …" : quote;
    input.value = "";
    ok.disabled = true;
    modal.hidden = false;
    input.focus();
    return new Promise((resolve) => {
      const done = (value) => {
        modal.hidden = true;
        modal.onkeydown = null;
        resolve(value);
      };
      input.oninput = () => (ok.disabled = !input.value.trim());
      ok.onclick = () => input.value.trim() && done(input.value);
      $("#annotate-cancel").onclick = () => done(null);
      modal.onclick = (e) => { if (e.target === modal) done(null); };
      modal.onkeydown = (e) => {
        if (e.key === "Escape") { e.preventDefault(); done(null); }
        else if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) { e.preventDefault(); ok.onclick(); }
        e.stopPropagation();
      };
    });
  }

  // Right-click on a selection: Annotate / Copy.
  const ctxMenu = $("#ctx-menu");
  if (/Mac/.test(navigator.platform)) {
    const keys = ctxMenu.querySelectorAll("kbd");
    keys[0].textContent = "⌥⌘M";
    keys[1].textContent = "⌘C";
  }
  let ctxTarget = null;
  const hideCtx = () => { ctxMenu.hidden = true; ctxTarget = null; };
  document.addEventListener("contextmenu", (e) => {
    hideCtx();
    // The webview's own menu (Back, Reload…) means nothing here; keep it only
    // for text fields, where it offers cut and paste.
    const field = e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement;
    const t = S.file && e.target.closest?.("#preview, #editor, #source-code") ? annotationTarget() : null;
    if (!t) {
      if (!field) e.preventDefault();
      return;
    }
    e.preventDefault();
    ctxTarget = t;
    ctxMenu.hidden = false;
    const w = ctxMenu.offsetWidth, h = ctxMenu.offsetHeight;
    ctxMenu.style.left = Math.min(e.clientX, window.innerWidth - w - 4) + "px";
    ctxMenu.style.top = Math.min(e.clientY, window.innerHeight - h - 4) + "px";
  });
  // Keep the selection while clicking the menu.
  ctxMenu.addEventListener("mousedown", (e) => e.preventDefault());
  $("#ctx-annotate").onclick = () => { const t = ctxTarget; hideCtx(); if (t) annotate(t); };
  $("#ctx-copy").onclick = () => { hideCtx(); document.execCommand("copy"); };
  document.addEventListener("mousedown", (e) => { if (!ctxMenu.hidden && !ctxMenu.contains(e.target)) hideCtx(); });
  document.addEventListener("keydown", (e) => { if (e.key === "Escape" && !ctxMenu.hidden) hideCtx(); }, true);
  window.addEventListener("blur", hideCtx);
  for (const pane of [previewPane, sourcePane]) pane.addEventListener("scroll", () => ctxMenu.hidden || hideCtx(), { passive: true });

  // Called by the app when the window is closing with unsaved edits.
  async function requestClose() {
    ipc("close_ack");
    if (!$("#modal").hidden) return; // already asking
    if (await confirmLeave()) ipc("quit");
  }

  editor.addEventListener("input", onEdit);
  for (const ev of ["keyup", "click", "select", "focus"]) editor.addEventListener(ev, scheduleCursor);
  document.addEventListener("selectionchange", () => {
    if (document.activeElement === editor) return scheduleCursor();
    if (sourceRaf) return;
    sourceRaf = requestAnimationFrame(() => {
      sourceRaf = 0;
      const sel = window.getSelection();
      const node = sel && sel.rangeCount ? sel.getRangeAt(0).commonAncestorContainer : null;
      if (node && preview.contains(node)) return syncPreviewSelection();
      clearSourceHighlight();
      if (!S.editing) updateSourceSelection();
    });
  });
  // While typing, the preview follows the editor.
  editor.addEventListener("keydown", () => (driver = "source"));
  editor.addEventListener("focus", () => { driver = "source"; hideEditorPick(); });
  editor.addEventListener("input", hideEditorPick);
  editor.addEventListener("keydown", (e) => {
    if (e.key === "Tab" && !e.ctrlKey && !e.metaKey && !e.altKey) {
      e.preventDefault();
      // insertText keeps the browser's undo history intact.
      if (!e.shiftKey) document.execCommand("insertText", false, "    ");
    }
  });

  window.mdview = { fsChanged, openPath, reply, requestClose };

  // ---------- keyboard ----------
  window.addEventListener("keydown", (e) => {
    const mod = e.ctrlKey || e.metaKey;
    const k = e.key.toLowerCase();
    const typing = e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement;
    if (!$("#modal").hidden || !$("#annotate").hidden) return;
    if (mod && e.altKey && k === "m") { e.preventDefault(); const t = annotationTarget(); if (t) annotate(t); else showToast("Select some text to annotate"); return; }
    if (mod && k === "b") { e.preventDefault(); toggleTree(); }
    else if (mod && k === "e") { e.preventDefault(); toggleSource(); }
    else if (mod && k === "f") { e.preventDefault(); openFind(); }
    else if (mod && k === "l") { e.preventDefault(); toggleTheme(); }
    else if (mod && k === "s") { e.preventDefault(); if (S.editing) save(); }
    else if (mod && k === "r") { e.preventDefault(); if (S.file) { S.sourceLoadedFor = null; openFile(S.file, { push: false, keepScroll: true }); } loadTree(); }
    else if (mod && (k === "w" || k === "q")) { e.preventDefault(); ipc("close"); }
    else if (mod && (k === "=" || k === "+")) { e.preventDefault(); S.zoom = Math.min(2.5, +(S.zoom + 0.1).toFixed(2)); applyZoom(); }
    else if (mod && k === "-") { e.preventDefault(); S.zoom = Math.max(0.5, +(S.zoom - 0.1).toFixed(2)); applyZoom(); }
    else if (mod && k === "0") { e.preventDefault(); S.zoom = 1; applyZoom(); }
    else if (typing && e.altKey) return;
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
  $("#btn-edit .ico").innerHTML = ICON.pencil;
  $("#find-prev").innerHTML = ICON.chevUp;
  $("#find-next").innerHTML = ICON.chevDown;
  $("#find-close").innerHTML = ICON.close;

  $("#btn-tree").onclick = toggleTree;
  $("#btn-source").onclick = toggleSource;
  $("#btn-edit").onclick = () => (S.editing ? leaveEdit() : enterEdit());
  $("#btn-save").onclick = save;
  $("#btn-discard").onclick = discardChanges;
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
    syncWatches();
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
