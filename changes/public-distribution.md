---
kind: tooling
design: 48
book: getting-started/installation.md, learn/quick-start.md, reference/cli.md
---
Kobra installs with one line and no sign-in: `curl -fsSL https://slavkobojanic.github.io/kobra/install.sh | sh` (on Windows, `irm https://slavkobojanic.github.io/kobra/install.ps1 | iex`), from the public GitHub releases, checked against their `SHA256SUMS`. `k toolchain install` and `k self update` need no token either (one in `GH_TOKEN`, `GITHUB_TOKEN` or `gh auth token` is sent to GitHub's API only, to lift its rate limit), and each toolchain comes with its book, which the new `k docs` opens offline; the book is also online at https://slavkobojanic.github.io/kobra/.
