// The K extension: starts `k lsp` and connects it to VS Code through
// vscode-languageclient. Highlighting comes from syntaxes/k.tmLanguage.json;
// everything else (diagnostics, quick fixes, navigation, rename, hover,
// completion, signature help, symbols and inlay hints) comes from the
// language server, which is the compiler's own analysis.
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
  context.subscriptions.push(
    vscode.commands.registerCommand("k.restartServer", async () => {
      if (client) {
        await client.stop();
      }
      client = createClient();
      await client.start();
    }),
    vscode.workspace.onDidChangeConfiguration(async (event) => {
      if (event.affectsConfiguration("k.server")) {
        await vscode.commands.executeCommand("k.restartServer");
      }
    }),
  );
  try {
    await client.start();
  } catch (error) {
    vscode.window.showErrorMessage(
      `Couldn't start the K language server (\`${vscode.workspace.getConfiguration("k").get("server.path") || "k"} lsp\`): ${error}. ` +
        "Install `k` or set `k.server.path`.",
    );
  }
}

/** Stops the language server. */
async function deactivate() {
  if (client) {
    await client.stop();
    client = undefined;
  }
}

module.exports = { activate, deactivate };
