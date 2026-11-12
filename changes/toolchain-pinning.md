---
kind: tooling
design: 48
book: reference/cli.md, reference/project-files.md, apps/upgrading.md
---
A project pins its toolchain: `k = "0.1"` in `k.toml` and the exact version in `k.lock`, with every command passed to the matching installed toolchain, `K_TOOLCHAIN` for one command, a format version in every lockfile, `@Deprecated` with machine-applicable fixes, and `k upgrade` to move a project to a newer toolchain.
