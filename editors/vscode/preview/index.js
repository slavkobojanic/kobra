// The preview panel (docs/design/57-previews.md, "The panel"): one
// `k preview --editor` per project that enables previews, the Kobra Preview
// webview beside the editor, the status bar item for a preview error, and
// the commands (`kobra.preview.open` is also the language server's
// **▶ Preview** CodeLens command). The protocol client and the pure
// helpers are in `client.js`, which has its own tests.

"use strict";

const path = require("node:path");
const crypto = require("node:crypto");
const vscode = require("vscode");
const {
  PreviewClient,
  SelectionTracker,
  fileKey,
  regionAt,
  parsePagePath,
  pageUrl,
  projectRootOf,
  throttle,
} = require("./client");

const VIEW_TYPE = "kobra.preview";

/** @param {vscode.TextDocument} document */
const isKobra = (document) => document.languageId === "k" && document.uri.scheme === "file";

/** `light` or `dark`, from the colour theme's kind, for the page's `theme=`. */
function themeName() {
  const kind = vscode.window.activeColorTheme.kind;
  return kind === vscode.ColorThemeKind.Light || kind === vscode.ColorThemeKind.HighContrastLight
    ? "light"
    : "dark";
}

/** What the panel shows: a client's page of one file. */
class Panel {
  /** @param {Previews} owner */
  constructor(owner) {
    this.owner = owner;
    /** @type {vscode.WebviewPanel | null} */
    this.webview = null;
    /** @type {PreviewClient | null} */
    this.client = null;
    /** The project path of the file shown, and the preview selected. */
    this.file = null;
    this.index = 0;
    this.grid = false;
    /** The iframe's current `src`, to load only on a change. */
    this.loaded = null;
    /** The origin the webview's CSP allows frames from. */
    this.origin = null;
  }

  get open() {
    return this.webview !== null;
  }

  /** Opens the panel beside the editor, keeping focus there. */
  create() {
    if (this.webview) return;
    this.webview = vscode.window.createWebviewPanel(
      VIEW_TYPE,
      "Kobra Preview",
      { viewColumn: vscode.ViewColumn.Beside, preserveFocus: true },
      // The page keeps its controls and mounts while the tab is hidden.
      { enableScripts: true, retainContextWhenHidden: true, localResourceRoots: [] },
    );
    this.webview.iconPath = new vscode.ThemeIcon("open-preview");
    this.webview.onDidDispose(() => {
      this.webview = null;
      this.loaded = null;
      this.origin = null;
      this.owner.panelClosed();
    });
    this.loaded = null;
    this.origin = null;
  }

  /**
   * Shows `file`'s previews of `client`, selecting `index`: switches the
   * loaded page by message when it is the same page, else loads it.
   * @param {PreviewClient} client
   * @param {string} file
   * @param {number} index
   */
  async show(client, file, index) {
    const sameClient = this.client === client;
    this.client = client;
    this.file = file;
    this.index = index;
    if (!this.webview) return;
    if (this.loaded && sameClient && client.url) {
      // The page is up: it switches without a reload, keeping its
      // environment bar.
      this.post({ kpreview: "select", file, index });
      return;
    }
    await this.reload();
  }

  /** Loads the page for the current client, file, theme and mode. */
  async reload() {
    const webview = this.webview;
    if (!webview) return;
    const client = this.client;
    if (!client || !client.url || this.file === null) {
      this.setHtml(null, client && !client.url ? "Starting k preview…" : "No previews");
      return;
    }
    // `asExternalUri` forwards the port in a remote window or Codespaces;
    // locally it is the same URL.
    const external = await vscode.env.asExternalUri(vscode.Uri.parse(client.url));
    if (this.webview !== webview) return;
    const base = external.toString(true);
    const src = pageUrl(base, this.file, this.index, { theme: themeName(), grid: this.grid });
    if (src === this.loaded) return;
    const origin = `${external.scheme}://${external.authority}`;
    if (origin !== this.origin) this.setHtml(origin, null);
    this.loaded = src;
    webview.webview.postMessage({ type: "load", src });
  }

  /** Posts a message into the embedded page. */
  post(/** @type {object} */ message) {
    if (this.webview) this.webview.webview.postMessage({ type: "post", message });
  }

  /**
   * The webview's document: an iframe filling it, allowed to load only
   * `origin`, and a script that loads and messages it.
   * @param {string | null} origin
   * @param {string | null} notice text shown instead of a page
   */
  setHtml(origin, notice) {
    if (!this.webview) return;
    this.origin = origin;
    if (origin === null) this.loaded = null;
    const nonce = crypto.randomBytes(16).toString("base64");
    const csp = [
      "default-src 'none'",
      `frame-src ${origin || "'none'"}`,
      `script-src 'nonce-${nonce}'`,
      "style-src 'unsafe-inline'",
    ].join("; ");
    const escape = (/** @type {string} */ s) => s.replace(/[&<>"]/g, (c) => `&#${c.charCodeAt(0)};`);
    this.webview.webview.html = `<!doctype html>
<html><head><meta charset="utf-8">
<meta http-equiv="Content-Security-Policy" content="${escape(csp)}">
<style>
  html, body { margin: 0; padding: 0; height: 100%; overflow: hidden; }
  iframe { border: 0; width: 100%; height: 100%; display: block; }
  p { font-family: var(--vscode-font-family); color: var(--vscode-descriptionForeground); padding: 0 16px; }
</style></head>
<body>${notice ? `<p>${escape(notice)}</p>` : `<iframe id="page" title="Kobra Preview" allow="clipboard-read; clipboard-write"></iframe>`}
<script nonce="${nonce}">
  const frame = document.getElementById("page");
  window.addEventListener("message", (event) => {
    const m = event.data;
    if (!frame || !m) return;
    if (m.type === "load" && frame.src !== m.src) frame.src = m.src;
    if (m.type === "post" && frame.contentWindow && frame.src) {
      frame.contentWindow.postMessage(m.message, new URL(frame.src).origin);
    }
  });
</script></body></html>`;
  }

  dispose() {
    if (this.webview) this.webview.dispose();
  }
}

/** The previews of the window: the processes, the panel and the status item. */
class Previews {
  /** @param {vscode.ExtensionContext} context */
  constructor(context) {
    this.context = context;
    /** project root → its client */
    this.clients = new Map();
    this.panel = new Panel(this);
    this.tracker = new SelectionTracker();
    /** Set when the user closes the panel: auto-open waits for a command. */
    this.dismissed = false;
    this.output = vscode.window.createOutputChannel("Kobra Preview");
    this.status = vscode.window.createStatusBarItem("kobra.preview.error", vscode.StatusBarAlignment.Left, 0);
    this.status.name = "Kobra Preview error";
    this.status.command = "kobra.preview.revealError";
    /** directory → its project root (or null), until a k.toml changes. */
    this.roots = new Map();
    const manifests = vscode.workspace.createFileSystemWatcher("**/k.toml");
    const forget = () => this.roots.clear();
    manifests.onDidCreate(forget);
    manifests.onDidChange(forget);
    manifests.onDidDelete(forget);
    this.select = throttle((/** @type {vscode.TextEditor} */ editor) => this.sendSelect(editor), 150);

    context.subscriptions.push(
      this.output,
      this.status,
      manifests,
      vscode.commands.registerCommand("kobra.preview.open", (arg) => this.open(arg)),
      vscode.commands.registerCommand("kobra.preview.toggleGrid", () => this.toggleGrid()),
      vscode.commands.registerCommand("kobra.preview.revealError", () => this.revealError()),
      vscode.workspace.onDidOpenTextDocument((d) => this.opened(d)),
      vscode.workspace.onDidChangeTextDocument((e) => {
        const client = this.clientOf(e.document, false);
        if (client) client.didChange(e.document.uri.toString(), e.document.getText());
      }),
      vscode.workspace.onDidCloseTextDocument((d) => {
        const client = this.clientOf(d, false);
        if (client) client.didClose(d.uri.toString());
      }),
      vscode.window.onDidChangeTextEditorSelection((e) => {
        if (isKobra(e.textEditor.document)) this.select(e.textEditor);
      }),
      vscode.window.onDidChangeActiveTextEditor((editor) => this.follow(editor)),
      vscode.window.onDidChangeActiveColorTheme(() => this.panel.reload()),
      vscode.workspace.onDidChangeConfiguration((event) => {
        if (event.affectsConfiguration("k.server")) this.restart();
      }),
    );
    for (const document of vscode.workspace.textDocuments) this.opened(document);
    this.follow(vscode.window.activeTextEditor);
  }

  /**
   * The client of the project `document` is in, started on first use when
   * `create`; null for a file of a project without previews.
   * @param {vscode.TextDocument} document
   * @param {boolean} create
   * @returns {PreviewClient | null}
   */
  clientOf(document, create = true) {
    if (!isKobra(document)) return null;
    const dir = path.dirname(document.uri.fsPath);
    let root = this.roots.get(dir);
    if (root === undefined) {
      const folder = vscode.workspace.getWorkspaceFolder(document.uri);
      root = projectRootOf(document.uri.fsPath, folder && folder.uri.fsPath);
      this.roots.set(dir, root);
    }
    if (!root) return null;
    let client = this.clients.get(root);
    if (!client && create) client = this.startClient(root);
    return client || null;
  }

  /** Starts `k preview --editor` for the project at `root`. */
  startClient(/** @type {string} */ root) {
    const config = vscode.workspace.getConfiguration("k");
    const command = config.get("server.path") || "k";
    const extraEnv = config.get("server.extraEnv") || {};
    const client = new PreviewClient({ command, root, env: { ...process.env, ...extraEnv } });
    this.clients.set(root, client);
    const name = path.basename(root);
    client.on("log", (line) => this.output.appendLine(`[${name}] ${line}`));
    client.on("ready", (url) => {
      this.output.appendLine(`[${name}] previews on ${url}`);
      if (this.panel.client === client) this.panel.reload();
    });
    client.on("previews", (uri, list) => this.previewsChanged(client, uri, list));
    client.on("built", () => this.updateStatus());
    client.on("error", () => this.updateStatus());
    client.on("exit", (code, signal) => {
      this.output.appendLine(`[${name}] k preview stopped (${signal || `exit ${code}`}); restarting`);
      this.updateStatus();
    });
    client.on("gaveUp", (tail) => {
      this.output.appendLine(`[${name}] k preview keeps stopping; not restarting it`);
      vscode.window
        .showErrorMessage(
          `\`${command} preview\` keeps stopping for ${name}${tail ? `: ${tail.split("\n").filter(Boolean).pop()}` : ""}.`,
          "Show Output",
        )
        .then((choice) => choice && this.output.show());
    });
    for (const document of vscode.workspace.textDocuments) {
      if (this.clientOf(document, false) === client) client.didOpen(document.uri.toString(), document.getText());
    }
    client.start();
    return client;
  }

  /** Stops every process; each starts again when a file needs it. */
  async restart() {
    const clients = [...this.clients.values()];
    this.clients.clear();
    this.roots.clear();
    await Promise.all(clients.map((c) => c.stop()));
    for (const document of vscode.workspace.textDocuments) this.opened(document);
    const active = vscode.window.activeTextEditor;
    if (this.panel.open && active) {
      this.panel.client = null;
      this.follow(active, true);
    }
    this.updateStatus();
  }

  /** A document opened: its project's process starts, and gets the buffer. */
  opened(/** @type {vscode.TextDocument} */ document) {
    const client = this.clientOf(document, true);
    if (client) client.didOpen(document.uri.toString(), document.getText());
  }

  /** The project path of `document` in `client`'s project. */
  fileOf(/** @type {PreviewClient} */ client, /** @type {vscode.TextDocument} */ document) {
    const list = client.previewsOf(document.uri.toString());
    const page = list.length > 0 ? parsePagePath(list[0].path) : null;
    if (page) return page.file;
    const root = fileKey(vscode.Uri.file(client.root).toString());
    return path.relative(root, fileKey(document.uri.toString())).split(path.sep).join("/");
  }

  /** The preview the cursor of `editor` is in, else the first. */
  indexAt(/** @type {PreviewClient} */ client, /** @type {vscode.TextEditor} */ editor) {
    const list = client.previewsOf(editor.document.uri.toString());
    const offset = editor.document.offsetAt(editor.selection.active);
    return Math.max(0, regionAt(list, offset));
  }

  /**
   * The active editor changed: a Kobra file with previews is shown in the
   * panel (opening it when `kobra.preview.autoOpen` allows). Another editor
   * leaves the panel on the last file.
   * @param {vscode.TextEditor | undefined} editor
   * @param {boolean} [force] show even if it is the same file
   */
  follow(editor, force = false) {
    this.updateStatus();
    if (!editor || !isKobra(editor.document)) return;
    const client = this.clientOf(editor.document, true);
    if (!client) return;
    const list = client.previewsOf(editor.document.uri.toString());
    if (list.length === 0) return;
    const file = this.fileOf(client, editor.document);
    if (!this.panel.open) {
      if (this.dismissed || !vscode.workspace.getConfiguration("kobra.preview").get("autoOpen", true)) return;
      this.panel.create();
    } else if (!force && this.panel.client === client && this.panel.file === file) {
      return;
    }
    this.tracker.reset(editor.document.uri.toString());
    this.panel.show(client, file, this.indexAt(client, editor));
  }

  /** `k` listed a file's previews: the active editor's may open the panel. */
  previewsChanged(/** @type {PreviewClient} */ client, /** @type {string} */ uri, /** @type {unknown[]} */ list) {
    const editor = vscode.window.activeTextEditor;
    if (!editor || list.length === 0 || fileKey(editor.document.uri.toString()) !== fileKey(uri)) return;
    if (this.clientOf(editor.document, false) !== client) return;
    if (!this.panel.open || this.panel.client !== client || this.panel.file !== this.fileOf(client, editor.document)) {
      this.follow(editor);
    }
  }

  /** Sends `select` for the cursor, when it may be in a preview. */
  sendSelect(/** @type {vscode.TextEditor} */ editor) {
    const client = this.clientOf(editor.document, false);
    if (!client) return;
    const uri = editor.document.uri.toString();
    const offset = editor.document.offsetAt(editor.selection.active);
    if (this.tracker.shouldSend(uri, offset, client.previewsOf(uri))) client.select(uri, offset);
  }

  /**
   * `kobra.preview.open`: from the CodeLens (`{uri, index, label}`), the
   * editor title (a `Uri`) or the command palette (the active editor).
   * @param {unknown} arg
   */
  async open(arg) {
    /** @type {vscode.Uri | undefined} */
    let uri;
    /** @type {number | undefined} */
    let index;
    if (arg instanceof vscode.Uri) uri = arg;
    else if (arg && typeof arg === "object" && typeof (/** @type {any} */ (arg).uri) === "string") {
      uri = vscode.Uri.parse(/** @type {any} */ (arg).uri);
      const i = Number(/** @type {any} */ (arg).index);
      index = Number.isInteger(i) && i >= 0 ? i : undefined;
    } else if (vscode.window.activeTextEditor) uri = vscode.window.activeTextEditor.document.uri;
    if (!uri) {
      vscode.window.showInformationMessage("Open a Kobra file with a `preview` to preview it.");
      return;
    }
    const document = await vscode.workspace.openTextDocument(uri);
    const client = this.clientOf(document, true);
    if (!client) {
      vscode.window.showInformationMessage(
        'This project doesn\'t enable previews: add `preview = ["view-previews"]` to the `[package]` section of its k.toml.',
      );
      return;
    }
    const editor = vscode.window.visibleTextEditors.find((e) => e.document === document);
    if (index === undefined) index = editor ? this.indexAt(client, editor) : 0;
    this.dismissed = false;
    const file = this.fileOf(client, document);
    if (this.panel.open) this.panel.webview.reveal(undefined, true);
    else this.panel.create();
    this.tracker.reset(uri.toString());
    await this.panel.show(client, file, index);
  }

  /** Switches the panel between one preview and the grid of the file. */
  async toggleGrid() {
    this.panel.grid = !this.panel.grid;
    if (this.panel.open) await this.panel.reload();
    else await this.open(undefined);
  }

  /** The client whose error the status item shows: the panel's, else the active file's. */
  currentClient() {
    if (this.panel.open && this.panel.client) return this.panel.client;
    const editor = vscode.window.activeTextEditor;
    return editor ? this.clientOf(editor.document, false) : null;
  }

  /** Shows the current client's error in the status bar, or hides it. */
  updateStatus() {
    const client = this.currentClient();
    const error = client && client.error;
    if (!error) {
      this.status.hide();
      return;
    }
    this.status.text = `$(warning) Preview: error${error.line ? ` at line ${error.line}` : ""}`;
    const where = error.uri ? `${path.basename(fileKey(error.uri))}${error.line ? `:${error.line}` : ""}: ` : "";
    this.status.tooltip = `${where}${error.message}\nThe preview keeps the last good build. Click to go to the error.`;
    this.status.show();
  }

  /** Opens the current error's file at its line and column. */
  async revealError() {
    const client = this.currentClient();
    const error = client && client.error;
    if (!error || !error.uri) return;
    const key = fileKey(error.uri);
    const open = vscode.workspace.textDocuments.find((d) => d.uri.scheme === "file" && fileKey(d.uri.toString()) === key);
    const document = open || (await vscode.workspace.openTextDocument(vscode.Uri.parse(error.uri)));
    const line = Math.max(0, (error.line || 1) - 1);
    const column = Math.max(0, (error.column || 1) - 1);
    const position = new vscode.Position(line, column);
    await vscode.window.showTextDocument(document, { selection: new vscode.Range(position, position) });
  }

  /** The user closed the panel: it stays closed until a command opens it. */
  panelClosed() {
    this.dismissed = true;
    this.updateStatus();
  }

  /** Stops every process (`shutdown`) and closes the panel. */
  async dispose() {
    this.select.cancel();
    this.panel.dispose();
    const clients = [...this.clients.values()];
    this.clients.clear();
    await Promise.all(clients.map((c) => c.stop()));
  }
}

module.exports = { Previews };
