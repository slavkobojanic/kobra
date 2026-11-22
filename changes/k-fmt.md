---
kind: tooling
design: 56
book: getting-started/commands.md, reference/cli.md, getting-started/editor-setup.md
---
`k fmt` formats Kobra code into its one canonical layout: the project or workspace, the paths named, or standard input (`-`); `--check` prints a diff and fails on unformatted files, and `--format json` reports each file. The language server formats documents and selections with it, and the VS Code extension formats on save.
