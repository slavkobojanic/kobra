//! `k new <name>`: creates an app in a new directory `<name>/`
//! (`docs/design/24-projects.md`, "Creating a project").
//!
//! The skeleton is the files of `templates/app/`, embedded in the binary,
//! with `{{name}}` (the project name), `{{title}}` (its display name,
//! `my-shop` → `My Shop`), `{{app}}` (the app declaration's name, in upper
//! camel case: `MyShop`) and `{{k}}` (this toolchain's minor version)
//! filled in. Every generated file is formatted and passes `k check`,
//! including its test (`tests/new.rs`). No lockfiles are written; the first
//! `k build` writes them.
//!
//! The directory may exist if it is empty; otherwise nothing is written.

use std::path::Path;
use std::process::ExitCode;

use crate::cli::NewArgs;

/// The app skeleton: each file's path in the new project and its template.
const APP: &[(&str, &str)] = &[
    ("k.toml", include_str!("../templates/app/k.toml.tmpl")),
    ("App.k", include_str!("../templates/app/App.k.tmpl")),
    (
        ".gitignore",
        include_str!("../templates/app/gitignore.tmpl"),
    ),
    (
        "src/home/Home.k",
        include_str!("../templates/app/src/home/Home.k.tmpl"),
    ),
    (
        "src/home/Home.test.k",
        include_str!("../templates/app/src/home/Home.test.k.tmpl"),
    ),
    (
        "src/theme/AppTheme.k",
        include_str!("../templates/app/src/theme/AppTheme.k.tmpl"),
    ),
];

/// Names the skeleton declares itself; an app of the same name gets an
/// `App` suffix (`k new home` declares `HomeApp`).
const TAKEN: &[&str] = &["Home", "AppTheme"];

/// Whether `name` is an app's project name: lower case ASCII letters,
/// digits and hyphens, starting with a letter, with no empty part between
/// hyphens and none at the end.
fn is_project_name(name: &str) -> bool {
    name.starts_with(|c: char| c.is_ascii_lowercase())
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && name.split('-').all(|part| !part.is_empty())
}

/// `my-shop` → `["My", "Shop"]`.
fn capitalised_parts(name: &str) -> impl Iterator<Item = String> + '_ {
    name.split('-').map(|part| {
        let mut chars = part.chars();
        chars.next().map_or_else(String::new, |first| {
            first.to_ascii_uppercase().to_string() + chars.as_str()
        })
    })
}

/// The app declaration's name: upper camel case, `App` appended when it
/// would clash with a declaration of the skeleton.
fn app_name(name: &str) -> String {
    let app: String = capitalised_parts(name).collect();
    if TAKEN.contains(&app.as_str()) {
        app + "App"
    } else {
        app
    }
}

/// The display name: `my-shop` → `My Shop`.
fn title(name: &str) -> String {
    capitalised_parts(name).collect::<Vec<_>>().join(" ")
}

/// This toolchain's minor version (`0.1`), the `k` a new project requires.
fn minor_version() -> &'static str {
    let version = env!("CARGO_PKG_VERSION");
    version.rsplit_once('.').map_or(version, |(minor, _)| minor)
}

/// `template` with its placeholders filled in for `name`.
fn render(template: &str, name: &str) -> String {
    template
        .replace("{{name}}", name)
        .replace("{{title}}", &title(name))
        .replace("{{app}}", &app_name(name))
        .replace("{{k}}", minor_version())
}

/// Runs `k new`: exit status 0 when the project was written, 2 for a name
/// or directory it can't use (nothing is written then), and 1 when a file
/// can't be written.
pub fn run(args: &NewArgs) -> ExitCode {
    let name = args.name.as_str();
    if !is_project_name(name) {
        eprintln!(
            "error: `{name}` isn't a valid project name: use lower case letters, digits and \
             hyphens, starting with a letter (`my-app`)"
        );
        return ExitCode::from(2);
    }
    let dir = Path::new(name);
    if dir.exists() {
        let empty = dir.is_dir()
            && std::fs::read_dir(dir).is_ok_and(|mut entries| entries.next().is_none());
        if !empty {
            eprintln!("error: `{name}` already exists and isn't empty; pick another name");
            return ExitCode::from(2);
        }
    }
    for (path, template) in APP {
        let target = dir.join(path);
        let written = target
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| std::fs::write(&target, render(template, name)));
        if let Err(e) = written {
            eprintln!("error: cannot write {}: {e}", target.display());
            return ExitCode::from(1);
        }
    }
    println!("Created the app `{name}` in {name}/. Start it with:");
    println!();
    println!("    cd {name}");
    println!("    k dev");
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_titles_and_app_names() {
        assert!(is_project_name("tip"));
        assert!(is_project_name("my-shop-2"));
        for bad in [
            "", "Tip", "2tip", "-tip", "tip-", "a--b", "a_b", "a.b", "a b",
        ] {
            assert!(!is_project_name(bad), "{bad:?}");
        }
        assert_eq!(title("my-shop"), "My Shop");
        assert_eq!(app_name("my-shop"), "MyShop");
        assert_eq!(app_name("shop-2-go"), "Shop2Go");
        assert_eq!(app_name("home"), "HomeApp");
        assert_eq!(app_name("app-theme"), "AppThemeApp");
    }

    #[test]
    fn every_placeholder_is_filled_in() {
        for (path, template) in APP {
            let text = render(template, "my-shop");
            assert!(!text.contains("{{"), "{path}: {text}");
        }
    }
}
