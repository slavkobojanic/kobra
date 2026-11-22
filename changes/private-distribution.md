---
kind: tooling
design: 48
book: getting-started/installation.md, learn/quick-start.md, reference/cli.md
---
Kobra installs from its private GitHub repository for the people who have access to it: sign in with `gh auth login`, then run `gh api repos/slavkobojanic/kobra/contents/scripts/install.sh -H 'Accept: application/vnd.github.raw' | sh` (on Windows, the same with `install.ps1 … | Out-String | iex`). `k toolchain install` and `k self update` download with `GH_TOKEN`, `GITHUB_TOKEN` or `gh auth token`, sent to GitHub's API only, and each toolchain comes with its book, which the new `k docs` opens (`k docs --path` prints where it is).
