---
kind: tooling
design: 48
book: reference/cli.md, getting-started/installation.md, apps/upgrading.md
---
`k toolchain install`, `list`, `remove` and `default` manage installed toolchains, and `k self update` installs the newest release and makes it the default; downloads are checked against the index's size and SHA-256, and an interrupted install leaves nothing behind. `install.sh` and `install.ps1` install K into `~/.k` in one line.
