// The editor side of `k preview --editor` (docs/compiler/preview.md, "The
// editor protocol"), without VS Code: the process and its restarts, the
// buffers it overlays, what it last said (the page's URL, each file's
// previews, the error), and the small pure helpers the panel needs. The
// VS Code glue is in `index.js`; this module is tested on its own with
// `node --test test/` against a fake `k preview` and, when one is built,
// the real one.
//
// Plain JavaScript and Node built-ins only: the extension has no build step.

"use strict";

const childProcess = require("node:child_process");
const fs = require("node:fs");
const path = require("node:path");
const { EventEmitter } = require("node:events");
const { fileURLToPath } = require("node:url");

/** Restart delays after a crash, in milliseconds; the last one repeats. */
const BACKOFF = [500, 1000, 2000, 5000, 10000, 30000];
/** A process that ran this long without crashing resets the backoff. */
const HEALTHY_MS = 60000;
/** Crashes in a row (each before `HEALTHY_MS`) after which it gives up. */
const MAX_CRASHES = 6;
/** How long `stop` waits for `shutdown` before killing the process. */
const STOP_GRACE_MS = 2000;

/**
 * The key two URIs of one file share: its real path. `k` canonicalizes the
 * project root (on macOS `/tmp` is `/private/tmp`), and VS Code keeps the
 * path the folder was opened by.
 * @param {string} uri a `file:` URI
 * @returns {string}
 */
function fileKey(uri) {
  let file;
  try {
    file = fileURLToPath(uri);
  } catch {
    return uri;
  }
  try {
    return fs.realpathSync.native(file);
  } catch {
    // A file that isn't on disk yet: its directory may still resolve.
    try {
      return path.join(fs.realpathSync.native(path.dirname(file)), path.basename(file));
    } catch {
      return file;
    }
  }
}

/**
 * The preview the cursor is in, as far as the editor can tell: the last
 * one whose keyword starts at or before `offset`, or -1 before the first.
 * Only `k` knows where a declaration ends, so this decides when to send
 * `select`, and `k` decides what it selects.
 * @param {{start: number}[]} list a `previews` message's list
 * @param {number} offset UTF-16 offset of the cursor
 * @returns {number} an index into `list`, or -1
 */
function regionAt(list, offset) {
  let found = -1;
  for (let i = 0; i < list.length; i++) {
    if (list[i].start <= offset) found = i;
  }
  return found;
}

/**
 * The file and index a preview's page path names
 * (`/p/src/shop/Counter.k/1` → `{file: "src/shop/Counter.k", index: 1}`).
 * @param {string} pagePath
 * @returns {{file: string, index: number} | null}
 */
function parsePagePath(pagePath) {
  const m = /^\/p\/(.+)\/(\d+)$/.exec(pagePath);
  return m ? { file: m[1], index: Number(m[2]) } : null;
}

/**
 * The URL of the page the panel embeds: `base` (the `ready` URL, or what
 * the editor forwarded it to), the page `/p/<file>/<index>`, and the
 * query `embed=1&theme=…[&mode=grid]`.
 * @param {string} base
 * @param {string} file a project path
 * @param {number} index
 * @param {{theme: "light" | "dark", grid?: boolean}} options
 * @returns {string}
 */
function pageUrl(base, file, index, options) {
  const root = base.endsWith("/") ? base.slice(0, -1) : base;
  const encoded = file.split("/").map(encodeURIComponent).join("/");
  const query = new URLSearchParams({ embed: "1", theme: options.theme });
  if (options.grid) query.set("mode", "grid");
  return `${root}/p/${encoded}/${index}?${query}`;
}

/**
 * The project `k preview` should serve for `file`: the nearest directory
 * at or above it (up to `stop`, inclusive) with a `k.toml`, if that
 * `k.toml` enables previews (`view-previews`). A file of a project that
 * doesn't has no previews (each would be K8108), so no process starts.
 * @param {string} file an absolute path
 * @param {string | undefined} stop the workspace folder, or undefined for the filesystem root
 * @returns {string | null}
 */
function projectRootOf(file, stop) {
  let dir = path.dirname(file);
  for (;;) {
    const manifest = path.join(dir, "k.toml");
    let text = null;
    try {
      text = fs.readFileSync(manifest, "utf8");
    } catch {
      // No manifest here: look further up.
    }
    if (text !== null) {
      return /["']view-previews["']/.test(text) ? dir : null;
    }
    const parent = path.dirname(dir);
    if (parent === dir || (stop && path.relative(stop, dir) === "")) return null;
    dir = parent;
  }
}

/**
 * Calls `fn` at most once per `ms`, with the latest arguments: the first
 * call runs at once, later ones within the window run once at its end.
 * @template {unknown[]} A
 * @param {(...args: A) => void} fn
 * @param {number} ms
 * @param {{setTimeout: typeof setTimeout, clearTimeout: typeof clearTimeout, now: () => number}} [clock]
 * @returns {((...args: A) => void) & {cancel: () => void}}
 */
function throttle(fn, ms, clock = { setTimeout, clearTimeout, now: Date.now }) {
  let last = -Infinity;
  /** @type {ReturnType<typeof setTimeout> | null} */
  let timer = null;
  /** @type {A | null} */
  let pending = null;
  const run = () => {
    timer = null;
    last = clock.now();
    const args = /** @type {A} */ (pending);
    pending = null;
    fn(...args);
  };
  const throttled = (/** @type {A} */ ...args) => {
    pending = args;
    if (timer !== null) return;
    const wait = last + ms - clock.now();
    if (wait <= 0) run();
    else timer = clock.setTimeout(run, wait);
  };
  throttled.cancel = () => {
    if (timer !== null) clock.clearTimeout(timer);
    timer = null;
    pending = null;
  };
  return throttled;
}

/**
 * One `k preview --editor` process for one project, restarted when it
 * crashes. It keeps the buffers it was sent and sends them again to each
 * new process, and remembers what the process last said.
 *
 * Events: `ready (url)` (each start), `built (build)`,
 * `previews (uri, list)`, `error ({uri, message, line, column})`,
 * `exit (code, signal)` (an unexpected stop; a restart follows unless it
 * gave up), `gaveUp (stderrTail)`, `log (line)` (standard error).
 */
class PreviewClient extends EventEmitter {
  /**
   * @param {object} options
   * @param {string} options.command the `k` executable
   * @param {string} options.root the project directory
   * @param {NodeJS.ProcessEnv} [options.env]
   * @param {typeof childProcess.spawn} [options.spawn]
   * @param {number[]} [options.backoff]
   * @param {number} [options.healthyMs]
   * @param {number} [options.maxCrashes]
   */
  constructor(options) {
    super();
    this.command = options.command;
    this.root = options.root;
    this.env = options.env || process.env;
    this.spawnFn = options.spawn || childProcess.spawn;
    this.backoff = options.backoff || BACKOFF;
    this.healthyMs = options.healthyMs === undefined ? HEALTHY_MS : options.healthyMs;
    this.maxCrashes = options.maxCrashes || MAX_CRASHES;
    /** @type {import("node:child_process").ChildProcessWithoutNullStreams | null} */
    this.child = null;
    /** The page's URL from the last `ready`, or null while (re)starting. */
    this.url = null;
    /** The last build number. */
    this.build = null;
    /** @type {{uri: string | null, message: string, line: number | null, column: number | null} | null} */
    this.error = null;
    /** fileKey → {uri, list}, from the last `previews` for each file. */
    this.previews = new Map();
    /** uri → text: the buffers the editor has open. */
    this.buffers = new Map();
    this.crashes = 0;
    this.stopped = false;
    /** @type {ReturnType<typeof setTimeout> | null} */
    this.restartTimer = null;
    /** The last lines of standard error, for a message when it gives up. */
    this.stderrTail = [];
  }

  /** Starts the process (again). */
  start() {
    if (this.stopped || this.child) return;
    const child = this.spawnFn(this.command, ["preview", "--editor", "--no-open", this.root], {
      cwd: this.root,
      env: this.env,
      stdio: ["pipe", "pipe", "pipe"],
    });
    this.child = child;
    const startedAt = Date.now();
    let out = "";
    child.stdout.setEncoding("utf8");
    child.stdout.on("data", (chunk) => {
      out += chunk;
      let nl;
      while ((nl = out.indexOf("\n")) >= 0) {
        const line = out.slice(0, nl).trim();
        out = out.slice(nl + 1);
        if (line) this.receive(line);
      }
    });
    let err = "";
    child.stderr.setEncoding("utf8");
    child.stderr.on("data", (chunk) => {
      err += chunk;
      let nl;
      while ((nl = err.indexOf("\n")) >= 0) {
        const line = err.slice(0, nl);
        err = err.slice(nl + 1);
        this.stderrTail.push(line);
        if (this.stderrTail.length > 20) this.stderrTail.shift();
        this.emit("log", line);
      }
    });
    // Writing to a process that died is reported on stdin; `exit` handles it.
    child.stdin.on("error", () => {});
    let exited = false;
    const onExit = (/** @type {number | null} */ code, /** @type {string | null} */ signal) => {
      if (exited) return;
      exited = true;
      if (this.child === child) this.child = null;
      this.url = null;
      if (this.stopped) return;
      this.crashes = Date.now() - startedAt >= this.healthyMs ? 1 : this.crashes + 1;
      this.emit("exit", code, signal);
      if (this.crashes >= this.maxCrashes) {
        this.emit("gaveUp", this.stderrTail.join("\n"));
        return;
      }
      const delay = this.backoff[Math.min(this.crashes - 1, this.backoff.length - 1)];
      this.restartTimer = setTimeout(() => {
        this.restartTimer = null;
        this.start();
      }, delay);
    };
    child.on("exit", onExit);
    child.on("error", (e) => {
      this.stderrTail.push(String(e && e.message ? e.message : e));
      onExit(null, null);
    });
  }

  /** Handles one line of the process's standard output. */
  receive(/** @type {string} */ line) {
    let m;
    try {
      m = JSON.parse(line);
    } catch {
      this.emit("log", `not a protocol message: ${line}`);
      return;
    }
    switch (m.type) {
      case "ready":
        this.url = m.url;
        for (const [uri, text] of this.buffers) this.write({ type: "didOpen", uri, text });
        this.emit("ready", m.url);
        break;
      case "built":
        this.build = m.build;
        this.error = null;
        this.emit("built", m.build);
        break;
      case "previews":
        this.previews.set(fileKey(m.uri), { uri: m.uri, list: Array.isArray(m.list) ? m.list : [] });
        this.emit("previews", m.uri, this.previews.get(fileKey(m.uri)).list);
        break;
      case "error":
        this.error = { uri: m.uri ?? null, message: String(m.message ?? ""), line: m.line ?? null, column: m.column ?? null };
        this.emit("error", this.error);
        break;
      default:
        this.emit("log", `unknown message: ${line}`);
    }
  }

  /** The previews `k` last listed for the file at `uri` (empty if none). */
  previewsOf(/** @type {string} */ uri) {
    const entry = this.previews.get(fileKey(uri));
    return entry ? entry.list : [];
  }

  /** Writes a message, if the process is up and has said `ready`. */
  write(/** @type {object} */ message) {
    if (!this.child || this.url === null) return false;
    this.child.stdin.write(`${JSON.stringify(message)}\n`);
    return true;
  }

  didOpen(/** @type {string} */ uri, /** @type {string} */ text) {
    this.buffers.set(uri, text);
    this.write({ type: "didOpen", uri, text });
  }

  didChange(/** @type {string} */ uri, /** @type {string} */ text) {
    this.buffers.set(uri, text);
    this.write({ type: "didChange", uri, text });
  }

  didClose(/** @type {string} */ uri) {
    if (!this.buffers.delete(uri)) return;
    this.write({ type: "didClose", uri });
  }

  select(/** @type {string} */ uri, /** @type {number} */ offset) {
    this.write({ type: "select", uri, offset });
  }

  /**
   * Stops for good: `shutdown`, then a kill if it hasn't exited in time.
   * @returns {Promise<void>}
   */
  stop() {
    this.stopped = true;
    if (this.restartTimer) clearTimeout(this.restartTimer);
    this.restartTimer = null;
    const child = this.child;
    this.child = null;
    this.url = null;
    if (!child) return Promise.resolve();
    return new Promise((resolve) => {
      if (child.exitCode !== null || child.signalCode !== null) {
        resolve();
        return;
      }
      const timer = setTimeout(() => child.kill(), STOP_GRACE_MS);
      child.once("exit", () => {
        clearTimeout(timer);
        resolve();
      });
      try {
        child.stdin.write(`${JSON.stringify({ type: "shutdown" })}\n`);
        child.stdin.end();
      } catch {
        child.kill();
      }
    });
  }
}

/**
 * Decides when a cursor move is worth a `select`: in a file with previews,
 * at or after the first preview's keyword (see `regionAt`), and not at the
 * offset last sent for that file. `k` then selects the preview only if the
 * cursor is inside its declaration, and tells the pages only when that
 * changes what they show, so repeating a `select` costs one line.
 */
class SelectionTracker {
  constructor() {
    /** fileKey → the offset last sent. */
    this.sent = new Map();
  }

  /**
   * @param {string} uri
   * @param {number} offset
   * @param {{start: number}[]} list the file's previews
   * @returns {boolean} whether to send `select`
   */
  shouldSend(uri, offset, list) {
    if (regionAt(list, offset) < 0) return false;
    const key = fileKey(uri);
    if (this.sent.get(key) === offset) return false;
    this.sent.set(key, offset);
    return true;
  }

  /** Forgets what was sent for `uri`, so the next move sends again. */
  reset(/** @type {string} */ uri) {
    this.sent.delete(fileKey(uri));
  }
}

module.exports = {
  PreviewClient,
  SelectionTracker,
  fileKey,
  regionAt,
  parsePagePath,
  pageUrl,
  projectRootOf,
  throttle,
  BACKOFF,
};
