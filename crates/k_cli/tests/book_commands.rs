//! Every `k …` command line in the book parses with the real command-line
//! definitions (`src/cli.rs`), without running it, so a renamed or removed
//! flag fails the book rather than a reader.
//!
//! * In `bash`, `sh` and `shell` blocks every line is a command line; in
//!   `console` blocks, the lines starting with `$ `. A line may hold several
//!   commands (`cd shop && k check`); each one whose program is `k` is
//!   checked. Environment assignments before it (`K_PORT=1 k dev`) are
//!   skipped.
//! * Inline code in prose that starts with `k ` and a subcommand-like word
//!   (`` `k check --no-tests` ``) is checked too, except in a section (the
//!   text under one heading) with a `**Planned**` marker. Inline code names
//!   a command, so it may leave out required arguments (`k migrate new`);
//!   a `<placeholder>` is one word, and `always|never` its first choice.
//! * A command for a planned feature is marked with a trailing `# planned`
//!   comment and must **not** parse, so the test notices when it is built.
//! * Probes: for a planned feature of `k_sema::features` that is a
//!   command, the command must not parse yet. When it does, the test fails
//!   with the list of Planned notes to remove.

#![allow(clippy::unwrap_used, clippy::panic)]

#[allow(dead_code, missing_docs, unused_qualifications)]
#[path = "../src/cli.rs"]
mod cli;

#[path = "../../k_sema/tests/support/mdbook.rs"]
mod mdbook;

use clap::Parser;
use clap::error::ErrorKind;

/// Pages whose inline code isn't checked: the design-status page lists what
/// isn't built, and the diagnostics reference shows mistakes on purpose.
const SKIPPED_PAGES: &[&str] = &["reference/design-status.md", "reference/diagnostics.md"];

/// A command for a planned feature: the feature's id and a command line
/// that parses once the feature is built.
const PROBES: &[(&str, &str)] = &[
    ("k-new-library", "k new --library github.com/acme/charts"),
    ("k-fmt", "k fmt"),
    ("k-doc", "k doc"),
    ("k-explain", "k explain K2406"),
    ("packages", "k add github.com/acme/charts@v1.0.0"),
    ("protobuf", "k proto export"),
];

/// Splits a shell line into words: whitespace separates them, single and
/// double quotes group them, a backslash escapes the next character.
/// Returns the commands (split at `&&`, `||`, `;` and `|`) and the trailing
/// comment, if any.
fn shell_words(line: &str) -> (Vec<Vec<String>>, Option<String>) {
    let mut commands = vec![Vec::new()];
    let mut word = String::new();
    let mut in_word = false;
    let mut chars = line.chars().peekable();
    let mut comment = None;
    let end_word = |word: &mut String, in_word: &mut bool, commands: &mut Vec<Vec<String>>| {
        if *in_word {
            commands.last_mut().unwrap().push(std::mem::take(word));
            *in_word = false;
        }
    };
    while let Some(c) = chars.next() {
        match c {
            '\'' => {
                in_word = true;
                for q in chars.by_ref() {
                    if q == '\'' {
                        break;
                    }
                    word.push(q);
                }
            }
            '"' => {
                in_word = true;
                while let Some(q) = chars.next() {
                    match q {
                        '"' => break,
                        '\\' => word.extend(chars.next()),
                        q => word.push(q),
                    }
                }
            }
            '\\' => {
                in_word = true;
                word.extend(chars.next());
            }
            '#' if !in_word => {
                comment = Some(chars.collect::<String>().trim().to_owned());
                break;
            }
            '&' | '|' | ';' => {
                end_word(&mut word, &mut in_word, &mut commands);
                while chars.peek().is_some_and(|&n| n == '&' || n == '|') {
                    chars.next();
                }
                commands.push(Vec::new());
            }
            c if c.is_whitespace() => end_word(&mut word, &mut in_word, &mut commands),
            c => {
                in_word = true;
                word.push(c);
            }
        }
    }
    end_word(&mut word, &mut in_word, &mut commands);
    commands.retain(|c| !c.is_empty());
    (commands, comment)
}

/// The `k` invocation of a command, without the environment assignments
/// before it; `None` when the program isn't `k`.
fn k_invocation(words: &[String]) -> Option<Vec<String>> {
    let start = words
        .iter()
        .position(|w| !(w.contains('=') && w.starts_with(|c: char| c.is_ascii_uppercase())))?;
    (words[start] == "k").then(|| words[start..].to_vec())
}

/// Whether `args` parses (help and version requests count as parsing),
/// or the error clap reports.
fn parses(args: &[String]) -> Result<(), String> {
    match cli::Cli::try_parse_from(args) {
        Ok(_) => Ok(()),
        Err(e) if matches!(e.kind(), ErrorKind::DisplayHelp | ErrorKind::DisplayVersion) => Ok(()),
        Err(e) => Err(e
            .to_string()
            .lines()
            .next()
            .unwrap_or_default()
            .trim_start_matches("error: ")
            .to_owned()),
    }
}

/// One command line found in the book.
struct Found {
    /// `page:line`.
    at: String,
    /// The `k` words.
    args: Vec<String>,
    /// Whether it is marked `# planned`.
    planned: bool,
    /// Whether it is inline code naming a command, which may leave out
    /// required arguments (`k migrate new`).
    inline: bool,
}

/// Whether a parse error only says that arguments are missing, which is
/// how prose names a command (`k migrate`, `k migrate new`).
fn only_missing_arguments(args: &[String]) -> bool {
    matches!(
        cli::Cli::try_parse_from(args).map_err(|e| e.kind()),
        Err(ErrorKind::MissingRequiredArgument
            | ErrorKind::MissingSubcommand
            | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand)
    )
}

/// The words of inline code: a `<placeholder with spaces>` and a
/// `"quoted text"` are one word each, and `always|never|auto` stands for
/// its first alternative.
fn inline_words(code: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut rest = code.trim();
    while !rest.is_empty() {
        let end = if rest.starts_with('<') {
            rest.find('>').map_or(rest.len(), |i| i + 1)
        } else if let Some(quoted) = rest.strip_prefix('"') {
            let close = quoted.find('"').map_or(rest.len(), |i| i + 2);
            words.push(rest[1..close.saturating_sub(1).max(1)].to_owned());
            rest = rest[close..].trim_start();
            continue;
        } else {
            rest.find(char::is_whitespace).unwrap_or(rest.len())
        };
        let word = rest[..end].replace('\\', "");
        let word = if word.starts_with('<') {
            word
        } else {
            word.split('|').next().unwrap_or("").to_owned()
        };
        words.push(word);
        rest = rest[end..].trim_start();
    }
    words
}

/// Every `k` command line of the book, by the rules in the module docs.
fn command_lines() -> Vec<Found> {
    let src = mdbook::book_src();
    let mut found = Vec::new();
    for path in mdbook::markdown_files(&src, &[]) {
        let page = path.strip_prefix(&src).unwrap().display().to_string();
        let text = std::fs::read_to_string(&path).unwrap();
        let doc = mdbook::parse(&text);
        for fence in &doc.fences {
            let lang = fence.lang();
            if !matches!(lang, "bash" | "sh" | "shell" | "console") {
                continue;
            }
            let mut pending = String::new();
            let mut start = 0;
            for (i, line) in fence.body.iter().enumerate() {
                let line = if lang == "console" {
                    match line.trim_start().strip_prefix("$ ") {
                        Some(command) => command,
                        None if !pending.is_empty() => line.as_str(),
                        None => continue,
                    }
                } else {
                    line.as_str()
                };
                if pending.is_empty() {
                    start = fence.line + 1 + i;
                }
                if let Some(continued) = line.trim_end().strip_suffix('\\') {
                    pending.push_str(continued);
                    pending.push(' ');
                    continue;
                }
                pending.push_str(line);
                let (commands, comment) = shell_words(&pending);
                pending.clear();
                let planned = comment
                    .as_deref()
                    .is_some_and(|c| c.split_whitespace().next() == Some("planned"));
                for command in commands {
                    if let Some(args) = k_invocation(&command) {
                        found.push(Found {
                            at: format!("{page}:{start}"),
                            args,
                            planned,
                            inline: false,
                        });
                    }
                }
            }
        }
        if SKIPPED_PAGES.contains(&page.as_str()) {
            continue;
        }
        // A Planned note covers its section: the section describes what
        // isn't built.
        let section_of = |line: usize| doc.headings.iter().rposition(|(at, _, _)| *at <= line);
        let planned_sections: Vec<Option<usize>> = doc
            .paragraphs
            .iter()
            .filter(|p| p.text.contains("**Planned"))
            .map(|p| section_of(p.line))
            .collect();
        for paragraph in &doc.paragraphs {
            if planned_sections.contains(&section_of(paragraph.line)) {
                continue;
            }
            for span in mdbook::code_spans(&paragraph.text) {
                let Some(rest) = span.strip_prefix("k ") else {
                    continue;
                };
                // `k check`, `k migrate new`: a lowercase word after `k`.
                if !rest.starts_with(|c: char| c.is_ascii_lowercase() || c == '-') {
                    continue;
                }
                found.push(Found {
                    at: format!("{page}:{}", paragraph.line),
                    args: inline_words(&span),
                    planned: false,
                    inline: true,
                });
            }
        }
    }
    found
}

#[test]
fn shell_words_split_like_a_shell() {
    let (commands, comment) =
        shell_words(r#"cd shop && K_PORT=1 k test --filter "Cart total" | tee x  # planned"#);
    assert_eq!(
        commands,
        [
            vec!["cd", "shop"],
            vec!["K_PORT=1", "k", "test", "--filter", "Cart total"],
            vec!["tee", "x"],
        ]
    );
    assert_eq!(comment.as_deref(), Some("planned"));
    assert_eq!(
        k_invocation(&commands[1]).unwrap(),
        ["k", "test", "--filter", "Cart total"]
    );
    assert!(k_invocation(&commands[0]).is_none());
    assert_eq!(
        inline_words(r#"k migrate new --rename <entity or field> "a b" --color always\|never"#),
        [
            "k",
            "migrate",
            "new",
            "--rename",
            "<entity or field>",
            "a b",
            "--color",
            "always"
        ]
    );
}

/// Every `k` command line in the book parses, and every `# planned` one
/// doesn't.
#[test]
fn every_k_command_in_the_book_parses() {
    let found = command_lines();
    assert!(
        found.len() > 50,
        "too few `k` command lines found ({})",
        found.len()
    );
    let mut problems = Vec::new();
    for line in &found {
        let shown = line.args.join(" ");
        match (parses(&line.args), line.planned) {
            (Err(_), false) if line.inline && only_missing_arguments(&line.args) => {}
            (Err(error), false) => problems.push(format!("{}: `{shown}`: {error}", line.at)),
            (Ok(()), true) => problems.push(format!(
                "{}: `{shown}` is marked `# planned` but parses: the command is built; remove the \
                 marker and its Planned note",
                line.at
            )),
            _ => {}
        }
    }
    assert!(
        problems.is_empty(),
        "{} `k` command line(s) in the book don't match the command-line definitions \
         (crates/k_cli/src/cli.rs):\n{}",
        problems.len(),
        problems.join("\n")
    );
}

/// The Planned notes of the book that name `id`, as `page:line`.
fn notes_naming(id: &str) -> Vec<String> {
    let src = mdbook::book_src();
    let mut at = Vec::new();
    for path in mdbook::markdown_files(&src, &[]) {
        let page = path.strip_prefix(&src).unwrap().display().to_string();
        let doc = mdbook::parse(&std::fs::read_to_string(&path).unwrap());
        for paragraph in &doc.paragraphs {
            let names = paragraph.text.match_indices("**Planned**").any(|(i, m)| {
                let after = &paragraph.text[i + m.len()..];
                let ids = after
                    .trim_start()
                    .strip_prefix('(')
                    .and_then(|r| r.split_once(')'))
                    .map_or_else(
                        || after.split_whitespace().next().unwrap_or("").to_owned(),
                        |(list, _)| list.to_owned(),
                    );
                ids.split(',')
                    .map(|s| s.trim().trim_end_matches([',', '.', ')']))
                    .any(|s| s == id)
            });
            if names {
                at.push(format!("{page}:{}", paragraph.line));
            }
        }
    }
    at
}

/// A planned command doesn't parse yet; once it does, its notes must go.
#[test]
fn planned_commands_are_not_built() {
    use k_sema::features::{self, Status};
    let mut problems = Vec::new();
    for &(id, command) in PROBES {
        let feature = features::lookup(id)
            .unwrap_or_else(|| panic!("probe for `{id}`, which isn't in k_sema::features"));
        if feature.status != Status::Planned {
            continue;
        }
        let args: Vec<String> = command.split_whitespace().map(str::to_owned).collect();
        if parses(&args).is_ok() {
            problems.push(format!(
                "`{command}` parses, so `{id}` is built: change its row in \
                 crates/k_sema/src/features.rs and remove the Planned notes with id `{id}` ({})",
                notes_naming(id).join(", ")
            ));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}
