//! The editor integrations in `editors/` stay in step with the compiler:
//!
//! * the VS Code TextMate grammar (`editors/vscode/syntaxes/k.tmLanguage.json`)
//!   is valid JSON whose every pattern compiles, and it highlights every
//!   hard and contextual keyword of the lexer's keyword table
//!   ([`SyntaxKind::ALL`]) with a keyword-like scope;
//! * the Vim syntax files (`editors/vim/syntax/k.vim`, shared by Neovim as
//!   `editors/nvim/syntax/k.vim`) are identical and list every keyword;
//! * the VS Code manifest registers the `k` language, the grammar and the
//!   language configuration, and starts `k lsp`;
//! * the VS Code extension registers the command of the **▶ Preview**
//!   CodeLens and runs `k preview --editor` for its preview panel;
//! * the JetBrains plugin (`editors/intellij`) bundles that same grammar and
//!   language configuration, copied from `editors/vscode` by its build with
//!   no copy of its own, registers `.k` and starts `k lsp`.
//!
//! The grammar's regular expressions are written in the subset that both
//! Oniguruma (which VS Code uses) and the `regex` crate understand, no
//! lookaround, so this test can evaluate them.

#![allow(clippy::unwrap_used, clippy::panic, clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};

use k_syntax::SyntaxKind;
use regex::Regex;
use serde_json::Value;

fn editors() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../editors")
}

/// Every keyword of the lexer's table: hard keywords, then contextual ones.
fn keywords() -> Vec<&'static str> {
    SyntaxKind::ALL
        .iter()
        .filter(|k| k.is_keyword() || k.is_contextual_keyword())
        .filter_map(|k| k.fixed_text())
        .collect()
}

/// A pattern of the grammar with a `match` regex and the scope it gives.
struct Rule {
    regex: Regex,
    scopes: Vec<String>,
}

/// Collects every `match` rule (with its name or capture names) and checks
/// that every `begin`/`end`/`match` compiles.
fn collect(value: &Value, out: &mut Vec<Rule>) {
    match value {
        Value::Object(map) => {
            for key in ["begin", "end"] {
                if let Some(Value::String(re)) = map.get(key) {
                    Regex::new(re).unwrap_or_else(|e| panic!("{key} {re:?} doesn't compile: {e}"));
                }
            }
            if let Some(Value::String(re)) = map.get("match") {
                let regex =
                    Regex::new(re).unwrap_or_else(|e| panic!("match {re:?} doesn't compile: {e}"));
                let mut scopes = Vec::new();
                if let Some(Value::String(name)) = map.get("name") {
                    scopes.push(name.clone());
                }
                if let Some(Value::Object(captures)) = map.get("captures") {
                    for capture in captures.values() {
                        if let Some(Value::String(name)) = capture.get("name") {
                            scopes.push(name.clone());
                        }
                    }
                }
                out.push(Rule { regex, scopes });
            }
            for v in map.values() {
                collect(v, out);
            }
        }
        Value::Array(items) => {
            for v in items {
                collect(v, out);
            }
        }
        _ => {}
    }
}

/// Whether a scope highlights a keyword.
fn keyword_scope(scope: &str) -> bool {
    [
        "keyword",
        "storage",
        "constant.language",
        "variable.language",
    ]
    .iter()
    .any(|p| scope.starts_with(p))
}

#[test]
fn the_textmate_grammar_highlights_every_keyword() {
    let path = editors().join("vscode/syntaxes/k.tmLanguage.json");
    let text = fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let grammar: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(grammar["scopeName"], "source.k");
    assert!(
        grammar["patterns"]
            .as_array()
            .is_some_and(|p| !p.is_empty())
    );
    let mut rules = Vec::new();
    collect(&grammar, &mut rules);
    let mut missing = Vec::new();
    for keyword in keywords() {
        // In context: a keyword followed by a space, as it is written.
        let sample = format!("{keyword} ");
        let highlighted = rules.iter().any(|rule| {
            rule.regex
                .find(&sample)
                .is_some_and(|m| m.start() == 0 && m.as_str().trim_end().starts_with(keyword))
                && rule.scopes.iter().any(|s| keyword_scope(s))
        });
        if !highlighted {
            missing.push(keyword);
        }
    }
    assert!(
        missing.is_empty(),
        "keywords the grammar doesn't highlight: {missing:?}"
    );
    // A keyword inside a longer name is not a keyword.
    let keyword_rules: Vec<&Rule> = rules
        .iter()
        .filter(|r| r.scopes.iter().any(|s| s.starts_with("keyword")))
        .collect();
    let all = keywords();
    for name in ["letter", "iffy", "format", "matching", "viewModel"] {
        for rule in &keyword_rules {
            if let Some(m) = rule.regex.find(name) {
                let prefix_keyword =
                    m.start() == 0 && m.end() < name.len() && all.contains(&m.as_str());
                assert!(
                    !prefix_keyword,
                    "`{name}` is highlighted as the keyword `{}`",
                    m.as_str()
                );
            }
        }
    }
}

#[test]
fn the_vim_syntax_files_list_every_keyword() {
    let vim = fs::read_to_string(editors().join("vim/syntax/k.vim")).unwrap();
    let nvim = fs::read_to_string(editors().join("nvim/syntax/k.vim")).unwrap();
    assert_eq!(
        vim, nvim,
        "editors/vim/syntax/k.vim and editors/nvim/syntax/k.vim differ"
    );
    let words: Vec<&str> = vim
        .lines()
        .filter(|l| l.trim_start().starts_with("syn keyword"))
        .flat_map(|l| l.split_whitespace().skip(3))
        .collect();
    let missing: Vec<&str> = keywords()
        .into_iter()
        .filter(|k| !words.contains(k))
        .collect();
    assert!(
        missing.is_empty(),
        "keywords k.vim doesn't list: {missing:?}"
    );
}

#[test]
fn the_vscode_manifest_wires_the_language_and_the_server() {
    let dir = editors().join("vscode");
    let manifest: Value =
        serde_json::from_str(&fs::read_to_string(dir.join("package.json")).unwrap()).unwrap();
    let language = &manifest["contributes"]["languages"][0];
    assert_eq!(language["id"], "k");
    assert!(
        language["extensions"]
            .as_array()
            .unwrap()
            .contains(&Value::from(".k"))
    );
    let configuration = language["configuration"].as_str().unwrap();
    let config: Value =
        serde_json::from_str(&fs::read_to_string(dir.join(configuration)).unwrap()).unwrap();
    assert_eq!(config["comments"]["lineComment"], "//");
    let grammar = &manifest["contributes"]["grammars"][0];
    assert_eq!(grammar["scopeName"], "source.k");
    assert!(dir.join(grammar["path"].as_str().unwrap()).is_file());
    assert!(manifest["dependencies"]["vscode-languageclient"].is_string());
    // `k fmt`'s layout on save, through the server's `textDocument/formatting`
    // (docs/design/56-formatter.md, "In editors").
    let defaults = &manifest["contributes"]["configurationDefaults"]["[k]"];
    assert_eq!(defaults["editor.formatOnSave"], true, "{defaults}");
    assert_eq!(defaults["editor.tabSize"], 4, "{defaults}");
    assert_eq!(defaults["editor.insertSpaces"], true, "{defaults}");
    let main = manifest["main"].as_str().unwrap();
    let extension = fs::read_to_string(dir.join(main)).unwrap();
    assert!(extension.contains("\"lsp\""), "the extension runs `k lsp`");
    // No build output or dependencies are committed (a lockfile may be).
    assert!(
        !dir.join("node_modules").exists()
            || fs::read_to_string(dir.join(".gitignore"))
                .unwrap()
                .contains("node_modules")
    );
}

/// The extension shows the version the server reports in `initialize`
/// (`serverInfo.version`, tested in `server.rs`) in a language status item,
/// so a user sees which toolchain the project's `k` delegated to
/// (docs/design/48-releases-and-versioning.md, "The VS Code extension").
#[test]
fn the_vscode_extension_shows_the_servers_version() {
    let extension = fs::read_to_string(editors().join("vscode/extension.js")).unwrap();
    for needle in ["createLanguageStatusItem", "initializeResult", "serverInfo"] {
        assert!(extension.contains(needle), "extension.js lacks `{needle}`");
    }
}

/// The **▶ Preview** CodeLens names a command the client must run
/// (`kobra.preview.open`, docs/compiler/lsp.md): the VS Code extension
/// contributes and registers it, and its panel runs `k preview --editor`
/// (docs/design/57-previews.md, "The panel"). The panel's own tests are
/// `node --test` in `editors/vscode` (docs/compiler/preview.md §6).
#[test]
fn the_vscode_extension_runs_the_preview_codelens_command() {
    let dir = editors().join("vscode");
    let manifest: Value =
        serde_json::from_str(&fs::read_to_string(dir.join("package.json")).unwrap()).unwrap();
    let commands: Vec<&str> = manifest["contributes"]["commands"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|c| c["command"].as_str())
        .collect();
    let lens = k_lsp::server::handlers::PREVIEW_COMMAND;
    assert!(commands.contains(&lens), "{commands:?} lacks {lens}");
    let properties = &manifest["contributes"]["configuration"]["properties"];
    assert_eq!(properties["kobra.preview.autoOpen"]["default"], true);
    let panel = fs::read_to_string(dir.join("preview/index.js")).unwrap();
    assert!(
        panel.contains(&format!("registerCommand(\"{lens}\"")),
        "preview/index.js doesn't register {lens}"
    );
    let client = fs::read_to_string(dir.join("preview/client.js")).unwrap();
    assert!(client.contains(r#"["preview", "--editor", "--no-open""#));
    // The tests aren't packaged.
    let ignore = fs::read_to_string(dir.join(".vscodeignore")).unwrap();
    assert!(ignore.lines().any(|l| l == "test/**"));
}

/// Every file below `dir`, skipping the Gradle build's output and caches.
fn plugin_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if path.is_dir() {
            if !matches!(
                name.as_str(),
                "build" | ".gradle" | ".intellijPlatform" | ".kotlin"
            ) {
                plugin_files(&path, out);
            }
        } else {
            out.push(path);
        }
    }
}

/// The JetBrains plugin takes its highlighting from the VS Code extension:
/// its Gradle build copies `editors/vscode`'s manifest, `syntaxes/` and
/// language configuration into the plugin as a TextMate bundle, so there is
/// one grammar (docs/design/13-tooling.md, "JetBrains IDEs").
#[test]
fn the_intellij_plugin_bundles_the_vscode_grammar() {
    let dir = editors().join("intellij");
    let build = fs::read_to_string(dir.join("build.gradle.kts")).unwrap();
    let start = build
        .find("tasks.register<Sync>(\"textmateBundle\")")
        .expect("build.gradle.kts registers the `textmateBundle` copy");
    let task = &build[start..];
    let task = &task[..task.find("\n}\n").unwrap()];
    assert!(
        task.contains("dir(\"../vscode\")"),
        "the bundle isn't copied from editors/vscode:\n{task}"
    );
    for file in [
        "\"package.json\"",
        "\"language-configuration.json\"",
        "\"syntaxes/**\"",
    ] {
        assert!(task.contains(file), "the bundle lacks {file}:\n{task}");
    }
    // What it copies exists, and names the grammar and the configuration.
    let vscode = editors().join("vscode");
    let manifest: Value =
        serde_json::from_str(&fs::read_to_string(vscode.join("package.json")).unwrap()).unwrap();
    let grammar = manifest["contributes"]["grammars"][0]["path"]
        .as_str()
        .unwrap();
    assert!(grammar.trim_start_matches("./").starts_with("syntaxes/"));
    assert_eq!(
        manifest["contributes"]["languages"][0]["configuration"],
        "./language-configuration.json"
    );
    // The bundle lands next to the plugin's jars, in every sandbox.
    assert!(
        build.contains("tasks.withType<PrepareSandboxTask>()")
            && build.contains("from(textmateBundle)"),
        "the bundle isn't put into the plugin's sandboxes"
    );
    // No grammar or language configuration of its own.
    let mut files = Vec::new();
    plugin_files(&dir, &mut files);
    let own: Vec<String> = files
        .iter()
        .filter(|p| {
            let name = p.file_name().unwrap().to_string_lossy();
            name.ends_with(".tmLanguage.json")
                || name.ends_with(".tmLanguage")
                || name == "language-configuration.json"
        })
        .map(|p| p.display().to_string())
        .collect();
    assert!(own.is_empty(), "the plugin keeps its own copy: {own:?}");

    // `.k` is a Kobra file, highlighted by the bundle, served by `k lsp`.
    let plugin = fs::read_to_string(dir.join("src/main/resources/META-INF/plugin.xml")).unwrap();
    for needle in [
        "extensions=\"k\"",
        "<textmate.bundleProvider",
        "<platform.lsp.integrationProvider",
        "<depends>com.intellij.modules.lsp</depends>",
        "<depends>org.jetbrains.plugins.textmate</depends>",
    ] {
        assert!(plugin.contains(needle), "plugin.xml lacks `{needle}`");
    }
    let lsp =
        fs::read_to_string(dir.join("src/main/kotlin/slavkobojanic/kobra/KobraLsp.kt")).unwrap();
    assert!(lsp.contains("\"lsp\""), "the plugin runs `k lsp`");
    // The Gradle wrapper is committed; its distribution is pinned by checksum.
    let wrapper = fs::read_to_string(dir.join("gradle/wrapper/gradle-wrapper.properties")).unwrap();
    assert!(wrapper.contains("distributionUrl=https\\://services.gradle.org/distributions/"));
    assert!(wrapper.contains("distributionSha256Sum="));
    assert!(
        dir.join("gradlew").is_file() && dir.join("gradle/wrapper/gradle-wrapper.jar").is_file()
    );
}
