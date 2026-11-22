<!--
The next release's highlights: three to six bullets, written by the release manager when the release
is prepared (docs/compiler/release.md §6), anything operators must know first (a new wire encoding
being written) at the top. `cargo xtask release prepare` puts them at the top of the release's
CHANGELOG.md section and empties this file at each release.
-->
- The first release of Kobra: one `k` binary that checks, tests, formats, runs and builds full-stack web
  apps, and a language server for editors. Nothing to migrate.
- `dist/app` is one self-contained server executable per app, for Docker, Compose and Kubernetes.
- A project pins its toolchain from now on: `k = "0.1"` in `k.toml`, the exact version in `k.lock`; a
  newer `k` hands the command to the pinned one, and `k upgrade` moves a project to a later minor.
- This release's wire formats (rpc encoding, service tokens, session cookies, job payloads) are
  recorded: 0.2 reads them, so a rolling deploy from 0.1 to 0.2 works, one minor at a time.
- Collaborators install from the private repository with the GitHub CLI: `gh auth login`, then
  `gh api repos/slavkobojanic/kobra/contents/scripts/install.sh -H 'Accept: application/vnd.github.raw' | sh`
  (or `install.ps1` on Windows), or `k toolchain install`; every download is checked against its
  SHA-256, and `k docs` opens the book installed with the toolchain. Kobra is MIT OR Apache-2.0.
