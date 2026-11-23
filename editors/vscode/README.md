# Kobra for Visual Studio Code

Syntax highlighting for `.k` files, the Kobra language server and the **Kobra Preview** panel. The server is `k lsp`, which ships in the
`k` binary: it is the compiler's own analysis, so the editor shows exactly what `k check` reports.

What you get:

- diagnostics as you type, with the compiler's notes and related locations, and **quick fixes** for every
  diagnostic that has a structured fix (`Ctrl+.` / `Cmd+.`);
- go to definition (into the standard library too), type definition and **implementation** (an interface or
  protocol to every type that implements it, an interface method to its implementations);
- find references and highlights, and **rename** across the project (imports, doc links, implementing methods
  and, for a public declaration, its file);
- hover with the declaration, its rendered documentation (links resolved), the **reactive dependencies** of a
  `@Derived` field or `@Effect`, and a page's URL;
- completion (names in scope, members after `.`, implicit members such as `padding: .md`, annotations after
  `@`), signature help, outline, workspace symbols and inlay hints with each page's derived URL;
- **formatting** with `k fmt`'s formatter (Format Document, Format Selection), **on save by default**: the
  extension sets `editor.formatOnSave`, four-space indentation and no indentation detection for Kobra files.
  To turn saving off, add `"[k]": { "editor.formatOnSave": false }` to your settings;
- the **toolchain's version** in the language status item (the `{}` next to "Kobra" in the status bar), as the
  server reports it: the `k` on `PATH` runs the project's pinned toolchain, so this is the version the
  project uses. Clicking it restarts the server;
- the **Kobra Preview** panel for `preview` declarations ([below](#previews)).

## Previews

In a project that enables previews (`preview = ["view-previews"]` in its `k.toml`), the extension runs
`k preview --editor` (the same `k` and environment as the language server, one process per project) and
shows each file's `preview` declarations in the **Kobra Preview** panel beside the editor:

- **It follows the editor.** Making a `.k` file with previews the active editor opens the panel beside it
  without taking focus (turn this off with `kobra.preview.autoOpen`); switching to another `.k` file with
  previews switches the panel to that file, and a non-Kobra editor leaves it on the last one. Moving the
  cursor into a `preview` selects it. Closing the panel keeps it closed until you open it again.
- **▶ Preview** above each `preview` (a CodeLens from `k lsp`), the **Open Preview** button in the editor
  title of a `.k` file and **Kobra: Open Preview** in the command palette open it on that preview.
- **Grid**: the layout button in the panel's title bar (**Kobra: Toggle Preview Grid**) shows every preview
  of the file at once.
- The page is `k preview`'s own panel (switcher, controls, environment bar), coloured to match the VS Code
  theme, light or dark. It rebuilds as you type, from the unsaved buffers.
- **Errors:** while the project doesn't type-check, the panel keeps the last good build under a badge, and
  the status bar shows **Preview: error at line N**; clicking it goes to the error.

**Remote windows.** `k preview` runs where the workspace is (an SSH host, a container, a Codespace), and
the panel reaches its page through VS Code's port forwarding (`asExternalUri`). Each preview itself renders
in a frame served from `127.0.0.1` on a port of its own, so in a remote window it shows only once VS Code
has forwarded that port to the same port number (automatic port forwarding usually does, on the desktop);
in VS Code for the Web (a Codespace in the browser) the frames don't load yet.

The process's log is in the **Kobra Preview** output channel. A `k preview` that keeps stopping is
restarted with a growing delay, then given up on with an error message.

## Install

The extension needs the `k` binary on `PATH` (or set `k.server.path`). Then, from this directory:

```sh
npm install                 # only vscode-languageclient
npx @vscode/vsce package    # builds kobra-0.1.0.vsix
code --install-extension kobra-0.1.0.vsix
```

For development, open this directory in VS Code and press `F5` to launch an Extension Development Host.

npm is used only here, for the extension. Building and testing the compiler never needs it.

## Settings

| Setting | Default | Meaning |
|---|---|---|
| `k.server.path` | `k` | The `k` executable; the server runs as `k lsp` |
| `k.server.extraEnv` | `{}` | Extra environment variables for the server |
| `k.trace.server` | `off` | Log the protocol messages in the output panel (`messages`, `verbose`) |
| `kobra.preview.autoOpen` | `true` | Open the preview panel when a `.k` file with previews becomes the active editor |

The command **Kobra: Restart Language Server** restarts `k lsp`, for example after installing a new `k`.

## Debugging

VS Code's built-in JavaScript debugger debugs Kobra as Kobra, with breakpoints in `.k` files, through the source
maps `k dev` serves. Nothing else needs installing. Copy [`launch.json`](launch.json) and
[`tasks.json`](tasks.json) into your project's `.vscode/` folder:

- **Kobra: debug in Chrome** starts `k dev --inspect` (the `k: dev` task) and opens the app in Chrome.
  Breakpoints in components, pages, services and event handlers stop in the browser.
- **Kobra: attach to the server (k dev --inspect)** attaches to the server's isolate on port 9229.
  Breakpoints in rpc handlers, `@Load` code running on the server and endpoint pages stop there.
- **Kobra: browser and server** does both at once.

The maps name the Kobra sources `/_k/src/<path>` in the browser and `k:/_k/src/<path>` on the server;
`sourceMapPathOverrides` maps both to the workspace folder, which must be the folder with `k.toml`
(change `${workspaceFolder}` to its subfolder otherwise). The runtime is on the maps' ignore list and in
`skipFiles`, so stepping stays in Kobra code. The book's Debugging chapter has the details, Firefox and
Safari included.

## How it fits together

- `syntaxes/k.tmLanguage.json` is the TextMate grammar. `crates/k_lsp/tests/editors.rs` checks that it
  highlights every keyword of the compiler's lexer, so the two can't drift apart.
- `language-configuration.json` sets comments, brackets, auto-closing pairs and `///` continuation.
- `extension.js` starts `k lsp` over standard input and output, and the preview panel.
- `preview/client.js` is the client of `k preview --editor` (the editor protocol of
  [`docs/compiler/preview.md`](../../docs/compiler/preview.md#5-the-editor-protocol)): the process and its
  restarts, the buffers, each file's previews and the error. It uses only Node's built-ins.
  `preview/index.js` is the VS Code side: the webview, the status bar item and the commands.
- `test/` tests both with Node's own runner, no `npm install` needed: `node --test` in this directory. They
  run against a fake `k preview` (`test/fake-k`) and a stub of the `vscode` module, and against the real
  `k preview --editor` on `crates/k_cli/tests/fixtures/previewing` when a `k` is built (`KOBRA_K`, else
  `target/debug/k` or `$CARGO_TARGET_DIR/debug/k`).

The protocol coverage, the server's architecture and its latency budgets are in
[`docs/compiler/lsp.md`](../../docs/compiler/lsp.md).
