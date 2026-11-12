// The K extension: starts `k lsp` and connects it to VS Code through
// vscode-languageclient. Highlighting comes from syntaxes/k.tmLanguage.json;
// everything else (diagnostics, quick fixes, navigation, rename, hover,
// completion, signature help, symbols and inlay hints) comes from the
// language server, which is the compiler's own analysis.
//
// The language status item shows the toolchain the server runs (the
// `serverInfo.version` of its `initialize` answer): the `k` on the PATH
// delegates to the project's pinned toolchain, so this is the version the
// project uses, whatever the extension's own version is.
//
// Plain JavaScript on purpose: the extension has no build step.

"use strict";

const vscode = require("vscode");
const { LanguageClient } = require("vscode-languageclient/node");

/**
 * The running client; replaced by `k.restartServer`, `undefined` after
 * deactivation.
 * @type {LanguageClient | undefined}
 */
let client;

/**
 * The language status item naming the running toolchain.
 * @type {vscode.LanguageStatusItem | undefined}
 */
let status;

/**
 * Starts `client` and shows the version it reports, or that it isn't
 * running.
 * @param {LanguageClient} started
 */
async function start(started) {
  if (status) {
    status.text = "K: starting…";
    status.busy = true;
  }
  try {
    await started.start();
  } finally {
    if (status) {
      status.busy = false;
    }
  }
  const info = started.initializeResult && started.initializeResult.serverInfo;
  if (status) {
    status.text = info && info.version ? `K ${info.version}` : "K";
    status.detail = "toolchain (k lsp)";
    status.severity = vscode.LanguageStatusSeverity.Information;
  }
}

/**
 * Creates (without starting) the client for the configured `k` executable:
 * `k.server.path lsp` over stdio, with `k.server.extraEnv` added to the
 * environment, for K files on disk.
 * @returns {LanguageClient}
 */
function createClient() {
  const config = vscode.workspace.getConfiguration("k");
  const command = config.get("server.path") || "k";
  const extraEnv = config.get("server.extraEnv") || {};
  // No `transport` field: stdio is the default, and setting
  // `TransportKind.stdio` would make the client append `--stdio`.
  const run = {
    command,
    args: ["lsp"],
    options: { env: { ...process.env, ...extraEnv } },
  };
  /** @type {import("vscode-languageclient/node").LanguageClientOptions} */
  const clientOptions = {
    documentSelector: [{ scheme: "file", language: "k" }],
    synchronize: {
      // The server reloads these when they change on disk (and registers
      // the same watchers itself when the client allows it).
      fileEvents: [
        vscode.workspace.createFileSystemWatcher("**/*.k"),
        vscode.workspace.createFileSystemWatcher("**/k.toml"),
        vscode.workspace.createFileSystemWatcher("**/*.lock"),
      ],
    },
  };
  return new LanguageClient("k", "K Language Server", { run, debug: run }, clientOptions);
}

/**
 * Starts the language server when the extension activates, and registers
 * the `k.restartServer` command, which a change to any `k.server.*` setting
 * also runs. A server that can't start is reported in an error message.
 * @param {vscode.ExtensionContext} context
 */
async function activate(context) {
  client = createClient();
  status = vscode.languages.createLanguageStatusItem("k.toolchain", { language: "k" });
  status.name = "K toolchain";
  status.command = { command: "k.restartServer", title: "Restart" };
  context.subscriptions.push(
    status,
    vscode.commands.registerCommand("k.restartServer", async () => {
      if (client) {
        await client.stop();
      }
      client = createClient();
      await start(client);
    }),
    vscode.workspace.onDidChangeConfiguration(async (event) => {
      if (event.affectsConfiguration("k.server")) {
        await vscode.commands.executeCommand("k.restartServer");
      }
    }),
  );
  try {
    await start(client);
  } catch (error) {
    if (status) {
      status.text = "K: not running";
      status.severity = vscode.LanguageStatusSeverity.Error;
    }
    vscode.window.showErrorMessage(
      `Couldn't start the K language server (\`${vscode.workspace.getConfiguration("k").get("server.path") || "k"} lsp\`): ${error}. ` +
        "Install `k` or set `k.server.path`.",
    );
  }
}

/** Stops the language server. */
async function deactivate() {
  status = undefined;
  if (client) {
    await client.stop();
    client = undefined;
  }
}

module.exports = { activate, deactivate };
