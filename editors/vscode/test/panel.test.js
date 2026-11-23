// The VS Code side of the preview panel (preview/index.js), on a stub of
// the `vscode` module and the fake `k preview` (test/fake-k): which
// process starts, when the panel opens, what the webview loads, the
// error status item and the commands. It doesn't launch VS Code.
//
//   cd editors/vscode && node --test

"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const Module = require("node:module");
const { EventEmitter } = require("node:events");
const { pathToFileURL, fileURLToPath } = require("node:url");

const FAKE = path.join(__dirname, "fake-k");

/** A small stand-in for the parts of the `vscode` API the panel uses. */
function stubVscode(settings) {
  const emitters = {};
  const event = (name) => {
    emitters[name] = emitters[name] || new EventEmitter();
    return (listener) => {
      emitters[name].on("fire", listener);
      return { dispose: () => emitters[name].off("fire", listener) };
    };
  };
  const fire = (name, value) => emitters[name] && emitters[name].emit("fire", value);

  class Uri {
    constructor(href) {
      this.href = href;
      const u = new URL(href);
      this.scheme = u.protocol.slice(0, -1);
      this.authority = u.host;
      this.fsPath = this.scheme === "file" ? fileURLToPath(href) : "";
    }
    static parse(href) {
      return new Uri(href);
    }
    static file(p) {
      return new Uri(pathToFileURL(p).href);
    }
    toString() {
      return this.href;
    }
  }

  const panels = [];
  const messages = [];
  const commands = new Map();
  const statusItems = [];
  const shown = [];
  const documents = [];

  const vscode = {
    Uri,
    ViewColumn: { Beside: -2 },
    ColorThemeKind: { Light: 1, Dark: 2, HighContrast: 3, HighContrastLight: 4 },
    StatusBarAlignment: { Left: 1 },
    ThemeIcon: class {
      constructor(id) {
        this.id = id;
      }
    },
    Position: class {
      constructor(line, character) {
        this.line = line;
        this.character = character;
      }
    },
    Range: class {
      constructor(start, end) {
        this.start = start;
        this.end = end;
      }
    },
    env: { asExternalUri: async (uri) => uri },
    commands: {
      registerCommand: (id, fn) => (commands.set(id, fn), { dispose() {} }),
    },
    window: {
      activeColorTheme: { kind: 2 },
      activeTextEditor: undefined,
      visibleTextEditors: [],
      createOutputChannel: () => ({ appendLine() {}, show() {}, dispose() {} }),
      createStatusBarItem: () => {
        const item = { text: "", visible: false, show() { this.visible = true; }, hide() { this.visible = false; }, dispose() {} };
        statusItems.push(item);
        return item;
      },
      createWebviewPanel: (viewType, title, showOptions, options) => {
        const disposed = new EventEmitter();
        const panel = {
          viewType,
          title,
          showOptions,
          options,
          webview: { html: "", postMessage: (m) => (messages.push(m), Promise.resolve(true)) },
          onDidDispose: (l) => disposed.on("dispose", l),
          reveal() {},
          dispose() {
            disposed.emit("dispose");
          },
        };
        panels.push(panel);
        return panel;
      },
      showInformationMessage: async () => undefined,
      showErrorMessage: async () => undefined,
      showTextDocument: async (document, options) => shown.push({ document, options }),
      onDidChangeTextEditorSelection: event("selection"),
      onDidChangeActiveTextEditor: event("activeEditor"),
      onDidChangeActiveColorTheme: event("theme"),
    },
    workspace: {
      textDocuments: documents,
      getConfiguration: (section) => ({ get: (key, fallback) => (settings[`${section}.${key}`] ?? fallback) }),
      getWorkspaceFolder: () => undefined,
      createFileSystemWatcher: () => ({ onDidCreate() {}, onDidChange() {}, onDidDelete() {}, dispose() {} }),
      openTextDocument: async (uri) => documents.find((d) => d.uri.toString() === uri.toString()),
      onDidOpenTextDocument: event("open"),
      onDidChangeTextDocument: event("change"),
      onDidCloseTextDocument: event("close"),
      onDidChangeConfiguration: event("configuration"),
    },
  };

  /** A text document and an editor on it, cursor at `offset`. */
  const open = (file, text, offset = 0) => {
    const document = {
      uri: Uri.file(file),
      languageId: file.endsWith(".k") ? "k" : "plaintext",
      getText: () => text,
      offsetAt: (p) => p.offset,
    };
    documents.push(document);
    const editor = { document, selection: { active: { offset } } };
    fire("open", document);
    return editor;
  };
  const activate = (editor) => {
    vscode.window.activeTextEditor = editor;
    vscode.window.visibleTextEditors = editor ? [editor] : [];
    fire("activeEditor", editor);
  };

  return { vscode, fire, open, activate, panels, messages, commands, statusItems, shown };
}

/** Loads preview/index.js with `stub` as its `vscode`. */
function loadPreviews(stub) {
  const original = Module._load;
  Module._load = function (request, ...rest) {
    if (request === "vscode") return stub;
    return original.call(this, request, ...rest);
  };
  try {
    delete require.cache[require.resolve("../preview/index.js")];
    return require("../preview/index.js").Previews;
  } finally {
    Module._load = original;
  }
}

async function until(check, what, ms = 10000) {
  const end = Date.now() + ms;
  for (;;) {
    const v = check();
    if (v) return v;
    if (Date.now() > end) throw new Error(`timed out waiting for ${what}`);
    await new Promise((r) => setTimeout(r, 20));
  }
}

const TEXT = 'component A {}\n\npreview {\n    A()\n}\n\npreview "big" {\n    A()\n}\n';

function project(t, previews = true) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "kobra-panel-"));
  t.after(() => fs.rmSync(dir, { recursive: true, force: true }));
  fs.writeFileSync(path.join(dir, "k.toml"), `[package]\nname = "a"\n${previews ? 'preview = ["view-previews"]\n' : ""}`);
  fs.mkdirSync(path.join(dir, "src"));
  return dir;
}

function start(t, settings = {}) {
  const stub = stubVscode({ "k.server.path": FAKE, ...settings });
  const Previews = loadPreviews(stub.vscode);
  const context = { subscriptions: [] };
  const previews = new Previews(context);
  t.after(() => previews.dispose());
  return { stub, previews };
}

test("a .k file with previews opens the panel beside the editor, on its page", async (t) => {
  const dir = project(t);
  const { stub, previews } = start(t);
  const editor = stub.open(path.join(dir, "src", "File.k"), TEXT, TEXT.indexOf('preview "big"') + 2);
  stub.activate(editor);
  assert.equal(previews.clients.size, 1, "one process for the project");
  await until(() => stub.messages.find((m) => m.type === "load"), "the page to load");
  assert.equal(stub.panels.length, 1);
  const panel = stub.panels[0];
  assert.equal(panel.title, "Kobra Preview");
  assert.deepEqual(panel.showOptions, { viewColumn: -2, preserveFocus: true });
  assert.equal(panel.options.enableScripts, true);
  const load = stub.messages.find((m) => m.type === "load");
  assert.equal(load.src, "http://127.0.0.1:4999/p/src/File.k/1?embed=1&theme=dark");
  assert.match(panel.webview.html, /frame-src http:\/\/127\.0\.0\.1:4999/);
  assert.match(panel.webview.html, /script-src 'nonce-/);

  // The grid, then a light theme, reload the page.
  await stub.commands.get("kobra.preview.toggleGrid")();
  assert.equal(stub.messages.at(-1).src, "http://127.0.0.1:4999/p/src/File.k/1?embed=1&theme=dark&mode=grid");
  stub.vscode.window.activeColorTheme.kind = 1;
  stub.fire("theme");
  await until(() => stub.messages.at(-1).src.includes("theme=light"), "the light theme");
});

test("a project that doesn't enable previews starts no process; autoOpen off keeps the panel closed", async (t) => {
  const plain = project(t, false);
  const { stub, previews } = start(t, { "kobra.preview.autoOpen": false });
  stub.activate(stub.open(path.join(plain, "src", "A.k"), TEXT));
  assert.equal(previews.clients.size, 0);
  const dir = project(t);
  stub.activate(stub.open(path.join(dir, "src", "File.k"), TEXT));
  assert.equal(previews.clients.size, 1);
  const client = [...previews.clients.values()][0];
  await until(() => client.previewsOf(stub.vscode.window.activeTextEditor.document.uri.toString()).length === 2, "the previews");
  assert.equal(stub.panels.length, 0, "autoOpen is off");
  // The CodeLens command opens it on the preview it names.
  await stub.commands.get("kobra.preview.open")({ uri: stub.vscode.window.activeTextEditor.document.uri.toString(), index: 1, label: "big" });
  await until(() => stub.messages.find((m) => m.type === "load"), "the page");
  assert.match(stub.messages.find((m) => m.type === "load").src, /\/p\/src\/File\.k\/1\?/);
});

test("following the editor: another .k file switches the page, other editors keep it", async (t) => {
  const dir = project(t);
  const { stub } = start(t);
  stub.activate(stub.open(path.join(dir, "src", "File.k"), TEXT));
  await until(() => stub.messages.find((m) => m.type === "load"), "the page");
  stub.activate(stub.open(path.join(dir, "notes.txt"), "hello"));
  stub.activate(undefined);
  const loads = stub.messages.length;
  const other = stub.open(path.join(dir, "src", "Other.k"), TEXT);
  stub.activate(other);
  await until(() => stub.messages.length > loads, "a switch");
  // The same process's page: a `select` into it rather than a reload.
  assert.deepEqual(stub.messages.at(-1), { type: "post", message: { kpreview: "select", file: "src/Other.k", index: 0 } });
  // Closing the panel keeps it closed when the next file opens.
  stub.panels[0].dispose();
  stub.activate(stub.open(path.join(dir, "src", "Third.k"), TEXT));
  await new Promise((r) => setTimeout(r, 200));
  assert.equal(stub.panels.length, 1);
});

test("an error shows in the status bar until the next build, and reveals its line", async (t) => {
  const dir = project(t);
  const { stub, previews } = start(t);
  const editor = stub.open(path.join(dir, "src", "File.k"), TEXT);
  stub.activate(editor);
  const client = [...previews.clients.values()][0];
  await until(() => client.build === 1, "the first build");
  const broken = TEXT.replace("A()\n}\n\npreview", "BROKEN\n}\n\npreview");
  stub.fire("change", { document: { ...editor.document, getText: () => broken } });
  const item = stub.statusItems[0];
  await until(() => item.visible, "the status item");
  assert.equal(item.text, "$(warning) Preview: error at line 4");
  await stub.commands.get("kobra.preview.revealError")();
  assert.equal(stub.shown.length, 1);
  assert.equal(stub.shown[0].document, editor.document);
  assert.equal(stub.shown[0].options.selection.start.line, 3);
  stub.fire("change", { document: editor.document });
  await until(() => !item.visible, "the status item to clear");
});

test("cursor moves in a preview send `select`, throttled", async (t) => {
  const dir = project(t);
  const log = path.join(dir, "received.jsonl");
  process.env.FAKE_K_LOG = log;
  t.after(() => delete process.env.FAKE_K_LOG);
  const { stub, previews } = start(t);
  const editor = stub.open(path.join(dir, "src", "File.k"), TEXT, 0);
  stub.activate(editor);
  const client = [...previews.clients.values()][0];
  await until(() => client.previewsOf(editor.document.uri.toString()).length === 2, "the previews");
  for (const offset of [1, TEXT.indexOf("preview {") + 1, TEXT.indexOf("preview {") + 2, TEXT.indexOf('preview "big"') + 3]) {
    editor.selection = { active: { offset } };
    stub.fire("selection", { textEditor: editor });
  }
  const selects = () =>
    fs.existsSync(log)
      ? fs.readFileSync(log, "utf8").split("\n").filter(Boolean).map((l) => JSON.parse(l)).filter((m) => m.type === "select")
      : [];
  await until(() => selects().length === 1 && selects()[0].offset === TEXT.indexOf('preview "big"') + 3, "the throttled select");
  await new Promise((r) => setTimeout(r, 300));
  assert.equal(selects().length, 1, "the throttle sent only the last position");
});
