# Kobra for JetBrains IDEs

A plugin for IntelliJ IDEA, WebStorm, PyCharm, GoLand, RustRover and the other IntelliJ-based IDEs,
2026.2 or later, free versions included. It gives `.k` files the `Kobra` file type, highlighting from the
VS Code extension's TextMate grammar, and the Kobra language server, `k lsp`, through the IDE's built-in
LSP support. Since the server is the compiler's own analysis, the IDE shows exactly what `k check`
reports.

What you get:

- diagnostics as you type, and **quick fixes** for every diagnostic with a structured fix (`Alt+Enter`);
- completion, Quick Documentation (hover), go to declaration and type declaration, Find Usages;
- **Rename** across the project (`Shift+F6`);
- **Reformat Code** with `k fmt`'s formatter (turn on **Reformat code** in Settings | Tools | Actions on
  Save to format on save);
- inlay hints, brace matching and Comment with Line/Block Comment.

IntelliJ IDEA open-source builds and Android Studio have no LSP support, so the plugin doesn't install
there.

## Install

Download `kobra-intellij-<version>.zip` from a release, then **Settings | Plugins | ⚙ | Install Plugin
from Disk…** and pick the zip (don't unpack it). The plugin needs the `k` binary (see the book's
Installation page): it looks on `PATH`, then in `~/.k/bin` (`$K_HOME/bin`). Without one it shows a
notification with the install command, and `.k` files are highlighted only.

## Build and test

Java 25 is needed (the IDE the plugin is built against runs on it); nothing else is installed by hand.
The Gradle wrapper downloads Gradle, the IntelliJ Platform Gradle Plugin and IntelliJ IDEA from Gradle's
and JetBrains' repositories.

```sh
./gradlew test            # the plugin in a headless IDE
./gradlew buildPlugin     # build/distributions/kobra-<version>.zip
./gradlew verifyPlugin    # the plugin verifier, against the same IDE
./gradlew runIde          # an IDE with the plugin, for trying it out
```

On macOS with several JDKs, run them with `JAVA_HOME=$(/usr/libexec/java_home -v 25)`.

## Settings

| Setting | Default | Meaning |
|---|---|---|
| Settings \| Languages & Frameworks \| Kobra \| Path to k | empty | The `k` executable; the server runs as `k lsp`. Empty: `k` on `PATH`, then `~/.k/bin/k`. A bare name is looked up on `PATH` |

**Tools | Restart Kobra Language Server** restarts `k lsp`, for example after installing a new `k`.

## How it fits together

- `build.gradle.kts` copies `../vscode/package.json`, `../vscode/syntaxes/` and
  `../vscode/language-configuration.json` into the plugin as a TextMate bundle (`textmate/kobra/` next to
  the plugin's jars). There is one grammar; `crates/k_lsp/tests/editors.rs` checks that this directory
  keeps no copy of it.
- `KobraLanguage.kt` and `KobraParserDefinition.kt`: the file type and a flat PSI (the whole file is one
  token). There is no parser: meaning comes from the server.
- `KobraHighlighting.kt`: TextMate's highlighter and brace matcher for the file type, with each bracket of
  the language configuration made a token of its own so the IDE can match it.
- `KobraTextMate.kt`: the bundle provider, and the commenter, from the language configuration's
  `comments`.
- `KobraLsp.kt`: starts `k lsp` over standard input and output for the project (the IntelliJ Platform's
  `LspIntegrationProvider`), the missing-`k` notification and the restart action.
- `KobraToolchain.kt`, `KobraSettings.kt`: finding `k`, and the setting.
- `gradle.properties`: `pluginVersion`, always the workspace version (`crates/k_cli/tests/version.rs`
  checks it, `cargo xtask release prepare` moves it), and the IDE version the plugin is built against.

Why the platform's LSP API rather than LSP4IJ is in
[`docs/design/13-tooling.md`](../../docs/design/13-tooling.md#jetbrains-ides), "JetBrains IDEs".
