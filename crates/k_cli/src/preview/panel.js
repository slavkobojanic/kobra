// The panel of `k preview` (docs/compiler/preview.md, "The page"): the
// switcher, the controls, the environment bar and the grid around the
// frames, over the server's JSON API and event stream. Plain JavaScript,
// no build step.
//
// URL: /p/<file>/<index>[?mode=grid][&embed=1][&theme=light|dark]
// From an embedding window (an editor's webview), the page accepts
// postMessage `{kpreview: "select", file, index}` and
// `{kpreview: "env", env: {...}}`.

const params = new URLSearchParams(location.search);
const embed = params.get("embed") === "1";
if (embed) document.body.classList.add("embed");
if (params.get("theme")) document.documentElement.dataset.theme = params.get("theme");

const $ = (id) => document.getElementById(id);
const el = (tag, attrs = {}, ...children) => {
  const e = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (k === "class") e.className = v;
    else if (k.startsWith("on")) e.addEventListener(k.slice(2), v);
    else if (v === true) e.setAttribute(k, "");
    else if (v !== false && v !== null && v !== undefined) e.setAttribute(k, String(v));
  }
  for (const c of children) if (c !== null && c !== undefined) e.append(c);
  return e;
};

/** Where the page is: a file and the index of a preview in it. */
const where = (() => {
  const m = /^\/p\/(.+)\/(\d+)$/.exec(decodeURIComponent(location.pathname));
  return m ? { file: m[1], index: Number(m[2]) } : { file: "", index: 0 };
})();
let mode = params.get("mode") === "grid" ? "grid" : "single";
let state = null;
let selectedLabel = null;

/** The environment bar's settings, kept for the tab. */
const env = (() => {
  try {
    return JSON.parse(sessionStorage.getItem("kpreview.env") ?? "{}");
  } catch {
    return {};
  }
})();
const saveEnv = () => {
  try {
    sessionStorage.setItem("kpreview.env", JSON.stringify(env));
  } catch {
    // No storage: the settings last as long as the page.
  }
};

/** The frames shown: slot → {group, iframe, frameBox, status, controls}. */
const slots = new Map();

const api = async (path, body) => {
  const r = await fetch(path, body === undefined ? {} : { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(body) });
  const v = await r.json().catch(() => ({}));
  if (!r.ok) throw Object.assign(new Error(v.error ?? `HTTP ${r.status}`), { body: v });
  return v;
};

const fileEntry = () => state?.files.find((f) => f.path === where.file) ?? null;
const previews = () => fileEntry()?.previews ?? [];
const current = () => previews()[where.index] ?? null;

function setWhere(file, index, push = false) {
  where.file = file;
  where.index = index;
  const q = new URLSearchParams(location.search);
  if (mode === "grid") q.set("mode", "grid");
  else q.delete("mode");
  const url = `/p/${file}/${index}${q.toString() ? `?${q}` : ""}`;
  if (push) history.pushState(null, "", url);
  else history.replaceState(null, "", url);
}

// ------------------------------------------------------------- the frames

/** The environment a frame gets: the bar's settings and the viewport. */
function frameEnv() {
  const [w, h] = viewportSize();
  return { scheme: env.scheme || null, reducedMotion: env.motion === true, width: w, height: h, locale: env.locale || null, offline: env.offline === true };
}

function viewportSize() {
  const v = env.viewport ?? "fit";
  if (v === "fit") return [null, null];
  if (v === "custom") return [Number(env.width) || 600, Number(env.height) || 800];
  const [w, h] = v.split("x").map(Number);
  return [w, h];
}

function sizeFrame(box) {
  const [w, h] = viewportSize();
  box.classList.toggle("fit", w === null);
  box.style.width = w === null ? "" : `${w}px`;
  box.style.height = w === null ? "" : `${h}px`;
}

/** The parameters' description a frame mounts with. */
const paramMeta = (p) => p.params.map((q) => ({ name: q.name, control: q.control.kind, cases: q.control.cases ?? [], type: q.control.type ?? null }));

/** Mounts preview `p` in slot `slot`: fresh containers, its current values. */
async function mount(slot, p) {
  const s = slots.get(slot);
  if (!s) return;
  s.group = p.group;
  s.status.textContent = "Mounting…";
  s.status.className = "status";
  try {
    const { url } = await api("/api/mount", { group: p.group, slot });
    const config = { values: p.values, params: paramMeta(p), env: frameEnv(), hold: env.hold === true };
    sizeFrame(s.frameBox);
    s.iframe.src = `${url}#${encodeURIComponent(JSON.stringify(config))}`;
  } catch (e) {
    s.status.textContent = `The preview couldn't start: ${e.message}`;
    s.status.className = "status failed";
  }
}

function newSlot(slot, title) {
  const iframe = el("iframe", { title });
  const frameBox = el("div", { class: "frame" }, iframe);
  const status = el("div", { class: "status", "aria-live": "polite" });
  const s = { group: null, iframe, frameBox, status, controls: null };
  slots.set(slot, s);
  return s;
}

window.addEventListener("message", (event) => {
  const m = event.data;
  if (!m || typeof m !== "object" || typeof m.kpreview !== "string") return;
  // From the embedding editor.
  if (event.source === window.parent && window.parent !== window) {
    if (m.kpreview === "select") select(m.file ?? where.file, Number(m.index) || 0);
    if (m.kpreview === "env") {
      Object.assign(env, m.env ?? {});
      saveEnv();
      syncEnvBar();
      envChanged(true);
    }
    return;
  }
  for (const [slot, s] of slots) {
    if (event.source !== s.iframe.contentWindow) continue;
    const p = state?.files.flatMap((f) => f.previews).find((x) => x.group === s.group);
    if (m.kpreview === "mounted") {
      s.status.textContent = m.title ? `Mounted · ${m.title}` : "Mounted";
      s.status.className = "status";
    } else if (m.kpreview === "failed") {
      s.status.textContent = m.message;
      s.status.className = "status failed";
    } else if (m.kpreview === "invalid") {
      s.controls?.invalid(new Set(m.names ?? []));
    } else if (m.kpreview === "values" && p) {
      // A `bind` wrote a parameter: the control follows, without a remount.
      const changed = Object.fromEntries(Object.entries(m.values ?? {}).filter(([, v]) => v !== undefined));
      api("/api/values", { group: p.group, values: changed }).then((r) => {
        p.values = r.values;
        for (const t of slots.values()) if (t.group === p.group) t.controls?.update(p.values);
      }, () => {});
    }
    void slot;
  }
});

// ------------------------------------------------------------- controls

/** The controls of preview `p`, which remount `remount` on a change. */
function controlsFor(p, remount) {
  const box = el("div", { class: "controls-list" });
  const inputs = new Map();
  let timer = null;
  const send = (name, value, later) => {
    clearTimeout(timer);
    const go = async () => {
      try {
        const r = await api("/api/values", { group: p.group, values: { [name]: value } });
        p.values = r.values;
        problem(name, null);
        remount();
      } catch (e) {
        problem(name, e.body?.errors?.[name] ?? e.message);
      }
    };
    if (later) timer = setTimeout(go, 300);
    else go();
  };
  const problem = (name, text) => {
    const c = inputs.get(name);
    if (!c) return;
    c.wrap.classList.toggle("invalid", text !== null);
    c.problem.textContent = text ?? "";
  };
  if (p.params.length === 0) box.append(el("p", { class: "status" }, "This preview has no parameters."));
  for (const q of p.params) {
    const kind = q.control.kind;
    const id = `c-${p.group}-${q.name}`;
    const value = p.values[q.name];
    let input;
    let read;
    let write;
    if (kind === "toggle") {
      input = el("input", { type: "checkbox", id });
      read = () => input.checked;
      write = (v) => (input.checked = v === true);
      input.addEventListener("change", () => send(q.name, read(), false));
    } else if (kind === "choice") {
      input = el("select", { id }, ...q.control.cases.map((c) => el("option", { value: c }, c)));
      read = () => input.value;
      write = (v) => (input.value = v ?? "");
      input.addEventListener("change", () => send(q.name, read(), false));
    } else if (kind === "integer" || kind === "number") {
      const step = kind === "integer" ? 1 : 0.1;
      const field = el("input", { type: "number", id, step: kind === "integer" ? "1" : "any", placeholder: q.default === null ? q.defaultSource : null });
      const bump = (d) => () => {
        const n = Number(field.value || q.default || 0) + d;
        field.value = kind === "integer" ? String(Math.round(n)) : String(Math.round(n * 1e6) / 1e6);
        send(q.name, Number(field.value), false);
      };
      input = el("span", { class: "stepper" }, el("button", { type: "button", "aria-label": `Decrease ${q.name}`, onclick: bump(-step) }, "−"), field, el("button", { type: "button", "aria-label": `Increase ${q.name}`, onclick: bump(step) }, "+"));
      read = () => (field.value === "" ? null : Number(field.value));
      write = (v) => (field.value = v === null || v === undefined ? "" : String(v));
      field.addEventListener("input", () => send(q.name, read(), true));
      input.field = field;
    } else {
      input = el("input", { type: "text", id, placeholder: q.default === null ? q.defaultSource : null });
      read = () => (input.value === "" && kind === "parsed" ? null : input.value);
      write = (v) => (input.value = v ?? "");
      input.addEventListener("input", () => send(q.name, read(), true));
    }
    write(value);
    const problemText = el("span", { class: "problem", "aria-live": "polite" });
    const wrap = el("div", { class: "control" }, el("label", { for: id }, el("span", { class: "name" }, q.name), " ", el("span", { class: "type" }, q.type)), input, problemText);
    inputs.set(q.name, { wrap, problem: problemText, write, input });
    box.append(wrap);
  }
  return {
    node: box,
    update(values) {
      for (const [name, c] of inputs) {
        const focused = c.wrap.contains(document.activeElement);
        if (!focused && Object.hasOwn(values, name)) c.write(values[name]);
      }
    },
    invalid(names) {
      for (const [name, c] of inputs) {
        const bad = names.has(name);
        c.wrap.classList.toggle("invalid", bad);
        c.problem.textContent = bad ? `this isn't a ${p.params.find((x) => x.name === name)?.type ?? "value"}; the default is shown` : "";
      }
    },
  };
}

// ------------------------------------------------------------- rendering

function render() {
  const main = $("main");
  main.textContent = "";
  slots.clear();
  $("file").textContent = where.file;
  const list = previews();
  const sw = $("switcher");
  sw.textContent = "";
  for (const [i, p] of list.entries()) sw.append(el("option", { value: String(i) }, p.label));
  sw.value = String(where.index);
  sw.disabled = mode === "grid";
  $("single").setAttribute("aria-pressed", String(mode === "single"));
  $("grid").setAttribute("aria-pressed", String(mode === "grid"));
  document.title = `${current()?.label ?? "Preview"} · ${where.file || "k preview"}`;
  if (list.length === 0) {
    main.append(el("p", { class: "empty" }, state?.build === null ? "Building…" : `No previews in ${where.file || "this file"}.`));
    return;
  }
  if (mode === "single") {
    const p = current() ?? list[0];
    selectedLabel = p.label;
    const s = newSlot("main", p.label);
    s.controls = controlsFor(p, () => mount("main", p));
    main.append(el("aside", { class: "controls" }, el("h2", {}, "Controls"), s.controls.node), el("section", { class: "stage" }, s.frameBox, s.status));
    mount("main", p);
  } else {
    const grid = el("section", { class: "grid" });
    for (const p of list) {
      const slot = `cell-${p.index}`;
      const s = newSlot(slot, p.label);
      s.controls = controlsFor(p, () => mount(slot, p));
      const pop = el("div", { class: "popover", hidden: true }, s.controls.node);
      const toggle = el("button", { type: "button", "aria-expanded": "false", title: "Controls", onclick: () => {
        pop.hidden = !pop.hidden;
        toggle.setAttribute("aria-expanded", String(!pop.hidden));
      } }, "Controls");
      if (p.params.length === 0) toggle.disabled = true;
      grid.append(el("article", { class: "cell" }, el("header", {}, el("span", { class: "label" }, p.label), toggle), pop, s.frameBox, s.status));
      mount(slot, p);
    }
    main.append(grid);
  }
}

function showError(error) {
  const b = $("badge");
  if (!error) {
    b.hidden = true;
    b.textContent = "";
    return;
  }
  b.hidden = false;
  const at = error.line ? `${error.file ?? ""}:${error.line}: ` : "";
  b.textContent = `${at}${error.message}${error.count > 1 ? ` (+${error.count - 1} more)` : ""} · showing the last good build`;
}

async function refresh() {
  state = await api("/api/state");
  // A rebuild keeps the preview by its label when it moved.
  const list = previews();
  if (selectedLabel !== null && list[where.index]?.label !== selectedLabel) {
    const i = list.findIndex((p) => p.label === selectedLabel);
    if (i >= 0) setWhere(where.file, i);
  }
  showError(state.error);
  render();
}

function select(file, index) {
  if (file === where.file && index === where.index && mode === "single") return;
  setWhere(file, index);
  selectedLabel = null;
  if (mode === "grid" && file === where.file) return;
  render();
}

// ------------------------------------------------------------- the bars

function syncEnvBar() {
  $("scheme").value = env.scheme ?? "";
  $("motion").checked = env.motion === true;
  $("viewport").value = env.viewport ?? "fit";
  $("custom").hidden = (env.viewport ?? "fit") !== "custom";
  $("width").value = env.width ?? 600;
  $("height").value = env.height ?? 800;
  $("locale").value = env.locale ?? "";
  $("offline").checked = env.offline === true;
  $("hold").checked = env.hold === true;
}

/** Applies a change of the bar: live in every frame, or by remounting. */
function envChanged(remount) {
  saveEnv();
  $("custom").hidden = (env.viewport ?? "fit") !== "custom";
  if (remount) {
    render();
    return;
  }
  for (const s of slots.values()) {
    sizeFrame(s.frameBox);
    s.iframe.contentWindow?.postMessage({ kpreview: "env", env: frameEnv() }, "*");
  }
}

$("scheme").addEventListener("change", (e) => ((env.scheme = e.target.value), envChanged(false)));
$("motion").addEventListener("change", (e) => ((env.motion = e.target.checked), envChanged(false)));
$("viewport").addEventListener("change", (e) => ((env.viewport = e.target.value), envChanged(false)));
$("width").addEventListener("change", (e) => ((env.width = Number(e.target.value)), envChanged(false)));
$("height").addEventListener("change", (e) => ((env.height = Number(e.target.value)), envChanged(false)));
$("locale").addEventListener("change", (e) => ((env.locale = e.target.value.trim()), envChanged(true)));
$("offline").addEventListener("change", (e) => ((env.offline = e.target.checked), envChanged(true)));
$("hold").addEventListener("change", (e) => ((env.hold = e.target.checked), envChanged(true)));
$("switcher").addEventListener("change", (e) => {
  setWhere(where.file, Number(e.target.value), true);
  selectedLabel = null;
  render();
});
$("single").addEventListener("click", () => {
  mode = "single";
  setWhere(where.file, where.index);
  render();
});
$("grid").addEventListener("click", () => {
  mode = "grid";
  setWhere(where.file, where.index);
  render();
});
$("reset").addEventListener("click", async () => {
  const targets = mode === "grid" ? previews() : [current()].filter(Boolean);
  for (const p of targets) {
    const r = await api("/api/values", { group: p.group, values: {}, reset: true });
    p.values = r.values;
  }
  render();
});
window.addEventListener("popstate", () => {
  const m = /^\/p\/(.+)\/(\d+)$/.exec(decodeURIComponent(location.pathname));
  if (m) {
    where.file = m[1];
    where.index = Number(m[2]);
    render();
  }
});

// ------------------------------------------------------------- live

const events = new EventSource("/api/events");
events.onmessage = (e) => {
  let m;
  try {
    m = JSON.parse(e.data);
  } catch {
    return;
  }
  if (m.type === "build") refresh();
  else if (m.type === "error") showError(m.error);
  else if (m.type === "select") select(m.file, m.index);
};
events.onopen = () => document.body.classList.remove("offline");
events.onerror = () => document.body.classList.add("offline");

syncEnvBar();
refresh();
