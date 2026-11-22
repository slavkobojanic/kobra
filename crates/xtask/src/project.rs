//! The project's name and where it is published, read from the workspace
//! `Cargo.toml`'s `[workspace.metadata.project]`
//! (`docs/design/48-releases-and-versioning.md`, "Owner decisions"): the one
//! source the version bump finds the toolchain image by, `release
//! attach-local` finds the repository by, and `crates/k_cli/tests/version.rs`
//! checks the workflows, Dockerfiles, editor extension and docs against.
//!
//! ```toml
//! [workspace.metadata.project]
//! name = "Kobra"
//! repository = "https://github.com/slavkobojanic/kobra"
//! site = "https://slavkobojanic.github.io/kobra/"
//! image = "ghcr.io/slavkobojanic/kobra"
//! extension = "slavkobojanic.kobra"
//! ```

use std::path::Path;

/// `[workspace.metadata.project]`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Project {
    /// The language's name in prose (`Kobra`).
    pub name: String,
    /// The source repository (`https://github.com/<owner>/<repo>`); its
    /// Releases host the archives.
    pub repository: String,
    /// The docs site, ending in `/` (GitHub Pages): the book, the install
    /// scripts and `index.json` (design 48, "Public distribution").
    pub site: String,
    /// The toolchain image without a tag (`ghcr.io/<owner>/<name>`).
    pub image: String,
    /// The VS Code extension's id (`<publisher>.<name>`).
    pub extension: String,
}

impl Project {
    /// Reads `root`'s `Cargo.toml`; a missing table or key reads as empty.
    #[must_use]
    pub fn read(root: &Path) -> Project {
        let text = std::fs::read_to_string(root.join("Cargo.toml")).unwrap_or_default();
        Project::parse(&text)
    }

    /// Parses the table out of a `Cargo.toml`'s text.
    #[must_use]
    pub fn parse(text: &str) -> Project {
        let mut project = Project::default();
        let mut inside = false;
        for line in text.lines() {
            let t = line.trim();
            if t.starts_with('[') {
                inside = t == "[workspace.metadata.project]";
                continue;
            }
            if !inside || t.starts_with('#') {
                continue;
            }
            let Some((key, value)) = t.split_once('=') else {
                continue;
            };
            let value = value.trim().trim_matches('"').to_owned();
            match key.trim() {
                "name" => project.name = value,
                "repository" => project.repository = value,
                "site" => project.site = value,
                "image" => project.image = value,
                "extension" => project.extension = value,
                _ => {}
            }
        }
        if !project.site.is_empty() && !project.site.ends_with('/') {
            project.site.push('/');
        }
        project
    }

    /// The repository as GitHub names it (`<owner>/<repo>`), for `gh -R`.
    #[must_use]
    pub fn github_repo(&self) -> Option<&str> {
        self.repository
            .strip_prefix("https://github.com/")
            .map(|r| r.trim_end_matches('/'))
            .filter(|r| r.contains('/'))
    }

    /// The image's last path segment (`kobra`), which a tag follows
    /// (`…/kobra:0.1.0`); `None` when no image is set.
    #[must_use]
    pub fn image_name(&self) -> Option<&str> {
        self.image.rsplit('/').next().filter(|n| !n.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::Project;

    #[test]
    fn the_table_is_read_and_the_repository_derived() {
        let p = Project::parse(
            "[workspace.package]\nname = \"x\"\n\n[workspace.metadata.project]\n# a note\nname = \"Kobra\"\nrepository = \"https://github.com/o/kobra\"\nsite = \"https://o.github.io/kobra\"\nimage = \"ghcr.io/o/kobra\"\nextension = \"o.kobra\"\n[other]\nname = \"y\"\n",
        );
        assert_eq!(p.name, "Kobra");
        assert_eq!(p.site, "https://o.github.io/kobra/");
        assert_eq!(p.github_repo(), Some("o/kobra"));
        assert_eq!(p.image_name(), Some("kobra"));
        assert_eq!(p.extension, "o.kobra");
        let none = Project::parse("[workspace]\n");
        assert_eq!(none.github_repo(), None);
        assert_eq!(none.image_name(), None);
    }
}
