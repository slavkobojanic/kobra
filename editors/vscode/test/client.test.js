// The preview client (preview/client.js) without VS Code:
//
//   cd editors/vscode && node --test
//
// The pure helpers, the protocol against a fake `k preview`
// (test/fake-k), and, when a `k` is built (KOBRA_K, else
// target/debug/k or $CARGO_TARGET_DIR/debug/k), the real `k preview
// --editor` on crates/k_cli/tests/fixtures/previewing.

"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const http = require("node:http");
const { pathToFileURL } = require("node:url");
const { once } = require("node:events");

const {
  PreviewClient,
  SelectionTracker,
  fileKey,
  regionAt,
  parsePagePath,
  pageUrl,
  projectRootOf,
  throttle,
} = require("../preview/client");

// No `.js`, so `node --test` doesn't take it for a test file.
const FAKE = path.join(__dirname, "fake-k");
const REPO = path.resolve(__dirname, "../../..");

/** A fresh temporary directory, removed after the test. */
function scratch(t) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "kobra-preview-"));
  t.after(() => fs.rmSync(dir, { recursive: true, force: true }));
  return dir;
}

/** The next `event` of `emitter`, or a failure after `ms`. */
async function next(emitter, event, ms = 10000) {
  const timeout = AbortSignal.timeout(ms);
  try {
    return await once(emitter, event, { signal: timeout });
  } catch (e) {
    if (timeout.aborted) throw new Error(`no \`${event}\` within ${ms} ms`);
    throw e;
  }
}

/** Waits until `check()` is truthy. */
async function until(check, what, ms = 10000) {
  const end = Date.now() + ms;
  for (;;) {
    const v = check();
    if (v) return v;
    if (Date.now() > end) throw new Error(`timed out waiting for ${what}`);
    await new Promise((r) => setTimeout(r, 20));
  }
}

const lines = (file) => (fs.existsSync(file) ? fs.readFileSync(file, "utf8").split("\n").filter(Boolean).map((l) => JSON.parse(l)) : []);

// ------------------------------------------------------------- helpers

test("regionAt is the last preview whose keyword starts at or before the cursor", () => {
  const list = [{ start: 10 }, { start: 50 }];
  assert.equal(regionAt(list, 0), -1);
  assert.equal(regionAt(list, 9), -1);
  assert.equal(regionAt(list, 10), 0);
  assert.equal(regionAt(list, 49), 0);
  assert.equal(regionAt(list, 50), 1);
  assert.equal(regionAt(list, 1000), 1);
  assert.equal(regionAt([], 5), -1);
});

test("parsePagePath reads the file and index of a page path", () => {
  assert.deepEqual(parsePagePath("/p/src/shop/Counter.k/1"), { file: "src/shop/Counter.k", index: 1 });
  assert.equal(parsePagePath("/"), null);
});

test("pageUrl embeds the page with the theme, and the grid on request", () => {
  assert.equal(
    pageUrl("http://127.0.0.1:4000/", "src/shop/Counter.k", 1, { theme: "dark" }),
    "http://127.0.0.1:4000/p/src/shop/Counter.k/1?embed=1&theme=dark",
  );
  assert.equal(
    pageUrl("https://x-4000.app.github.dev", "src/My Views/A.k", 0, { theme: "light", grid: true }),
    "https://x-4000.app.github.dev/p/src/My%20Views/A.k/0?embed=1&theme=light&mode=grid",
  );
});

test("projectRootOf finds the nearest k.toml, and only if it enables previews", (t) => {
  const dir = scratch(t);
  fs.writeFileSync(path.join(dir, "k.toml"), '[workspace]\nmembers = ["apps/*"]\n');
  const app = path.join(dir, "apps", "shop");
  fs.mkdirSync(path.join(app, "src"), { recursive: true });
  fs.writeFileSync(path.join(app, "k.toml"), '[package]\nname = "shop"\npreview = ["view-previews"]\n');
  const plain = path.join(dir, "apps", "plain");
  fs.mkdirSync(path.join(plain, "src"), { recursive: true });
  fs.writeFileSync(path.join(plain, "k.toml"), '[package]\nname = "plain"\n');
  assert.equal(projectRootOf(path.join(app, "src", "A.k"), dir), app);
  assert.equal(projectRootOf(path.join(plain, "src", "A.k"), dir), null);
  assert.equal(projectRootOf(path.join(dir, "Loose.k"), dir), null);
  // Not above the workspace folder.
  assert.equal(projectRootOf(path.join(app, "src", "A.k"), path.join(app, "src")), null);
});

test("fileKey gives one key to a file under a symlinked directory", (t) => {
  const dir = scratch(t);
  fs.writeFileSync(path.join(dir, "A.k"), "");
  const link = path.join(dir, "..", `${path.basename(dir)}-link`);
  fs.symlinkSync(dir, link);
  t.after(() => fs.unlinkSync(link));
  assert.equal(fileKey(pathToFileURL(path.join(link, "A.k")).href), fileKey(pathToFileURL(path.join(dir, "A.k")).href));
  // A file not on disk yet keys by its directory's real path.
  assert.equal(fileKey(pathToFileURL(path.join(link, "New.k")).href), path.join(fs.realpathSync.native(dir), "New.k"));
});

test("throttle runs at once, then once per window with the latest arguments", () => {
  let now = 0;
  const timers = [];
  const clock = {
    now: () => now,
    setTimeout: (fn, ms) => (timers.push({ fn, at: now + ms }), timers.length),
    clearTimeout: () => {},
  };
  const calls = [];
  const f = throttle((x) => calls.push(x), 100, clock);
  f(1);
  assert.deepEqual(calls, [1]);
  now = 20;
  f(2);
  f(3);
  assert.deepEqual(calls, [1], "within the window: later");
  assert.equal(timers.length, 1);
  now = timers[0].at;
  timers[0].fn();
  assert.deepEqual(calls, [1, 3]);
  now = 500;
  f(4);
  assert.deepEqual(calls, [1, 3, 4]);
});

test("SelectionTracker sends a cursor at or after a preview, once per offset", () => {
  const tracker = new SelectionTracker();
  const uri = "file:///nowhere/A.k";
  const list = [{ start: 10 }, { start: 50 }];
  assert.equal(tracker.shouldSend(uri, 5, list), false, "before the first preview");
  assert.equal(tracker.shouldSend(uri, 12, list), true);
  assert.equal(tracker.shouldSend(uri, 12, list), false, "the same offset");
  assert.equal(tracker.shouldSend(uri, 60, list), true);
  assert.equal(tracker.shouldSend(uri, 60, []), false, "a file without previews");
  tracker.reset(uri);
  assert.equal(tracker.shouldSend(uri, 60, list), true, "after a reset");
});

// ------------------------------------------------------------- the fake k

/** A client on the fake, with its own log of what the fake received. */
function fakeClient(t, env = {}, options = {}) {
  const dir = scratch(t);
  const log = path.join(dir, "received.jsonl");
  const client = new PreviewClient({
    command: FAKE,
    root: dir,
    env: { ...process.env, FAKE_K_LOG: log, FAKE_K_CRASH_FILE: path.join(dir, "runs"), ...env },
    ...options,
  });
  t.after(() => client.stop());
  return { client, dir, log };
}

const TEXT = 'component A {}\n\npreview {\n    A()\n}\n\npreview "big" (size: Int = 2) {\n    A()\n}\n';

test("the client sends buffers opened before `ready`, and keeps each file's previews", async (t) => {
  const { client, dir, log } = fakeClient(t);
  const uri = pathToFileURL(path.join(dir, "A.k")).href;
  client.didOpen(uri, TEXT);
  const ready = next(client, "ready");
  client.start();
  assert.deepEqual(await ready, ["http://127.0.0.1:4999/"]);
  const [got, list] = await next(client, "previews");
  assert.equal(got, uri);
  assert.deepEqual(
    list.map((p) => [p.index, p.label, p.line, p.start]),
    [
      [0, "Unnamed", 3, TEXT.indexOf("preview {")],
      [1, "big", 7, TEXT.indexOf('preview "big"')],
    ],
  );
  assert.equal(client.previewsOf(uri).length, 2);
  assert.equal(client.build, 1);
  assert.deepEqual(lines(log), [{ type: "didOpen", uri, text: TEXT }]);
});

test("an error is kept until the next build, and select and didClose are sent", async (t) => {
  const { client, dir, log } = fakeClient(t);
  const uri = pathToFileURL(path.join(dir, "A.k")).href;
  client.start();
  await next(client, "ready");
  client.didOpen(uri, TEXT);
  await next(client, "built");
  const broken = TEXT.replace("    A()\n}\n\npreview", "    BROKEN\n}\n\npreview");
  client.didChange(uri, broken);
  const [error] = await next(client, "error");
  assert.deepEqual(error, { uri, message: "cannot find `BROKEN` in scope", line: 4, column: 1 });
  assert.deepEqual(client.error, error);
  assert.equal(client.previewsOf(uri).length, 2, "the last good list stays");
  client.didChange(uri, TEXT);
  await next(client, "built");
  assert.equal(client.error, null);
  client.select(uri, 30);
  client.didClose(uri);
  client.didClose(uri); // not open any more: nothing is sent
  await until(() => lines(log).length === 5, "five messages");
  assert.deepEqual(
    lines(log).slice(3).map((m) => m.type),
    ["select", "didClose"],
  );
  assert.equal(lines(log)[3].offset, 30);
});

test("stop sends shutdown and the process exits", async (t) => {
  const { client, log } = fakeClient(t);
  client.start();
  await next(client, "ready");
  const child = client.child;
  let exitedOn = null;
  child.on("exit", (code) => (exitedOn = code));
  await client.stop();
  assert.equal(exitedOn, 0);
  assert.deepEqual(lines(log), [{ type: "shutdown" }]);
  assert.equal(client.url, null);
});

test("a crashed process restarts after a backoff and gets the buffers again", async (t) => {
  const { client, dir, log } = fakeClient(t, { FAKE_K_CRASH: "2" }, { backoff: [20, 40] });
  const uri = pathToFileURL(path.join(dir, "A.k")).href;
  client.didOpen(uri, TEXT);
  const exits = [];
  client.on("exit", (code) => exits.push(code));
  client.start();
  await until(() => exits.length === 2 && client.url, "two crashes and a third start");
  assert.deepEqual(exits, [3, 3]);
  await until(() => client.build === 1, "the third run's build");
  // The crashed runs may have read the buffer before exiting; the last one did.
  assert.deepEqual(lines(log).at(-1), { type: "didOpen", uri, text: TEXT });
});

test("a process that keeps failing is given up on, with its last words", async (t) => {
  const { client } = fakeClient(t, { FAKE_K_FAIL: "1" }, { backoff: [5], maxCrashes: 3 });
  const gaveUp = next(client, "gaveUp");
  client.start();
  const [tail] = await gaveUp;
  assert.match(tail, /unrecognized subcommand 'preview'/);
  assert.equal(client.child, null);
});

test("a missing executable is a crash, not a thrown error", async (t) => {
  const dir = scratch(t);
  const client = new PreviewClient({ command: path.join(dir, "no-such-k"), root: dir, backoff: [5], maxCrashes: 2 });
  t.after(() => client.stop());
  const gaveUp = next(client, "gaveUp");
  client.start();
  const [tail] = await gaveUp;
  assert.match(tail, /ENOENT/);
});

// ------------------------------------------------------------- the real k

function realK() {
  const candidates = [
    process.env.KOBRA_K,
    process.env.CARGO_TARGET_DIR && path.join(process.env.CARGO_TARGET_DIR, "debug", "k"),
    path.join(REPO, "target", "debug", "k"),
  ];
  return candidates.find((c) => c && fs.existsSync(c)) || null;
}

function getJson(url) {
  return new Promise((resolve, reject) => {
    http
      .get(url, (res) => {
        let body = "";
        res.setEncoding("utf8");
        res.on("data", (c) => (body += c));
        res.on("end", () => {
          try {
            resolve(JSON.parse(body));
          } catch (e) {
            reject(e);
          }
        });
      })
      .on("error", reject);
  });
}

test("against the real `k preview --editor` on the previewing fixture", { timeout: 120000 }, async (t) => {
  const k = realK();
  if (!k) {
    t.skip("no `k` built (set KOBRA_K, or cargo build -p k_cli)");
    return;
  }
  const dir = scratch(t);
  const root = path.join(dir, "previewing");
  fs.cpSync(path.join(REPO, "crates/k_cli/tests/fixtures/previewing"), root, { recursive: true });
  assert.equal(projectRootOf(path.join(root, "src/shop/Counter.k"), dir), root);
  const client = new PreviewClient({ command: k, root });
  t.after(() => client.stop());
  const file = path.join(root, "src/shop/Counter.k");
  // The URI VS Code would send, through the unresolved temporary path.
  const uri = pathToFileURL(file).href;
  const text = fs.readFileSync(file, "utf8");
  client.didOpen(uri, text);
  client.start();
  const [url] = await next(client, "ready", 60000);
  assert.match(url, /^http:\/\/127\.0\.0\.1:\d+\/$/);
  await until(() => client.previewsOf(uri).length === 2, "Counter.k's previews", 60000);
  const list = client.previewsOf(uri);
  assert.deepEqual(
    list.map((p) => [p.label, p.start, p.path]),
    [
      ["Counter", text.indexOf("preview ("), "/p/src/shop/Counter.k/0"],
      ["setup throws", text.indexOf('preview "setup throws"'), "/p/src/shop/Counter.k/1"],
    ],
  );
  assert.deepEqual(parsePagePath(list[1].path), { file: "src/shop/Counter.k", index: 1 });

  // The cursor in the second preview selects it on every page.
  const offset = list[1].start + 3;
  const tracker = new SelectionTracker();
  assert.ok(tracker.shouldSend(uri, offset, list));
  client.select(uri, offset);
  const state = await (async () => {
    const end = Date.now() + 20000;
    for (;;) {
      const s = await getJson(`${url}api/state`);
      if (s.selected || Date.now() > end) return s;
      await new Promise((r) => setTimeout(r, 50));
    }
  })();
  assert.deepEqual(state.selected, { file: "src/shop/Counter.k", index: 1 });

  // A broken edit: an error at its line; the fix builds again and clears it.
  const broken = text.replace("Counter(value: bind value, label: label)", "Counter(value: bind value, label: nope)");
  assert.notEqual(broken, text);
  client.didChange(uri, broken);
  const [error] = await next(client, "error", 60000);
  assert.match(error.message, /nope/);
  assert.equal(error.line, text.slice(0, text.indexOf("label: label)")).split("\n").length);
  assert.equal(fileKey(error.uri), fileKey(uri));
  client.didChange(uri, text);
  await next(client, "built", 60000);
  assert.equal(client.error, null);

  const child = client.child;
  const exited = once(child, "exit");
  await client.stop();
  const [code] = await exited;
  assert.equal(code, 0);
});
