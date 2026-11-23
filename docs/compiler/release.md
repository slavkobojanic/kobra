# Releasing Kobra

The maintainer runbook: how a version of Kobra is cut, checked, published and fixed. The decisions behind it
(what a version promises, channels, pinning, upgrades, formats, the docs) are in
[docs/design/48-releases-and-versioning.md](../design/48-releases-and-versioning.md); this file is the
procedure.

> **Status.** There are no tags and no published artifacts, and the workspace version is `0.1.0`.
> Every step of the [implementation plan](#12-implementation-plan) is built, and step 15, the
> `0.1.0-rc.1` dry run of §6, has been run locally (its results and what only the owner can do are in
> [step 15](#12-implementation-plan)): version consistency,
> the toolchain requirement, `k.lock`'s toolchain with delegation, `k toolchain` and `k self update`,
> lockfile format versions, `k fix`, the stability annotations with the feature table, `k upgrade`, the
> changelog fragments with `cargo xtask`, the 0.1 wire fixtures, the install scripts, the versioned docs
> build (`cargo xtask docs build`), the CI workflows in `.github/` and the toolchain version in the
> editor, then the dry run's fixes. **CI runs only on request**: every workflow's one trigger is
> `workflow_dispatch`, started from the Actions page or with `gh workflow run`
> ([§9, "Running CI"](#running-ci)), so pushing commits, branches or tags runs nothing, and the release
> workflow creates its own tag. Sections
> marked *(planned)* describe files and commands not built yet. The name (Kobra), the namespace
> (`slavkobojanic/kobra` on GitHub and GHCR) and the licence (MIT OR Apache-2.0) are decided
> ([design 48](../design/48-releases-and-versioning.md#owner-decisions)), and so is **public
> distribution**: the repository is public, anyone installs from its GitHub releases with
> `curl -fsSL https://slavkobojanic.github.io/kobra/install.sh | sh` (no sign-in, no token), the docs site
> is GitHub Pages with the install scripts and `index.json` at its root (each release also attaches its
> book, opened offline with `k docs`), and no CI job runs on macOS (the macOS archive is attached from the
> owner's Mac, §6) ([design 48, "Public distribution"](../design/48-releases-and-versioning.md#public-distribution-decided-2026-11-23); it supersedes the
> [private distribution](../design/48-releases-and-versioning.md#private-distribution-decided-2026-11-23)
> decided earlier that day).
> Publishing waits only for the owner to switch it on ([step 15](#12-implementation-plan)'s list), and
> every step up to publishing can be run locally.

Contents:

1. [Vocabulary](#1-vocabulary)
2. [Files the release process owns](#2-files-the-release-process-owns)
3. [For contributors: changelog fragments](#3-for-contributors-changelog-fragments)
4. [For contributors: breaking and format changes](#4-for-contributors-breaking-and-format-changes)
5. [Branches, tags and versions](#5-branches-tags-and-versions)
6. [Cutting a minor release](#6-cutting-a-minor-release)
7. [Cutting a patch release](#7-cutting-a-patch-release)
8. [Nightly](#8-nightly)
9. [The pipeline](#9-the-pipeline)
10. [After publishing](#10-after-publishing)
11. [When a release is bad](#11-when-a-release-is-bad)
12. [Implementation plan](#12-implementation-plan)

---

## 1. Vocabulary

| Term | Meaning |
|---|---|
| **minor** | `0.6`: the compatibility unit before 1.0; may break, always with a migration |
| **patch** | `0.6.2`: fixes only, no new diagnostics, no format change |
| **release candidate** | `0.6.0-rc.1`: the branch head a week before `0.6.0` |
| **nightly** | `0.7.0-nightly.20261109`: main on that date, after the nightly gate |
| **release branch** | `release/0.6`: where `0.6.*` and its docs fixes are made |
| **the gate** | every check of [design 48](../design/48-releases-and-versioning.md#the-gate), green on one commit |
| **release manager** | the maintainer running this procedure for one release |

---

## 2. Files the release process owns

| Path | What | Status |
|---|---|---|
| `Cargo.toml` `[workspace.package] version` | **the** version; everything else follows it | exists |
| `editors/vscode/package.json` `version` | the extension's version, equal to the toolchain's | exists |
| `editors/intellij/gradle.properties` `pluginVersion` | the JetBrains plugin's version, equal to the toolchain's | exists |
| `changes/*.md` | changelog fragments, one per change; `changes/README.md` explains them | exists (the 0.1.0 fragments, one per area) |
| `changes/_highlights.md` | hand-written release highlights, emptied at each release | exists |
| `CHANGELOG.md` | every released version's notes, newest first | exists (no version released yet) |
| `crates/xtask/` | `cargo xtask release prepare`, `check`, `start-next`, `nightly`, `attach-local`; `docs-gate`, `fragments`; `docs build`; `dist archive`, `docs`, `checksums`, `index`, `merge-index`, `smoke`; `ci changed`, `version`, `notes`, `check-version` | exists (`release sync-main` and `release yank` planned) |
| `.github/workflows/ci.yml`, `nightly.yml`, `release.yml`, `publish.yml` | the pipeline (§9), on GitHub Actions (owner decision, 2026-11-09); each run only when started (`workflow_dispatch`, owner decision 2026-11-23); checked by `crates/xtask/tests/workflows.rs` | exist |
| `.github/actions/setup/`, `.github/actions/site/` | the workflows' shared steps: Rust, mdBook and the caches; assembling the whole docs site | exist |
| `scripts/ci/` | the few steps that need `gh`, `git` or `cosign`: `release-index.sh`, `tag.sh` (the release tag), `publish-release.sh`, `sign.sh`, `verify.sh`, `nightly-issue.sh` | exist |
| `docker/toolchain.Dockerfile` | the toolchain image, from a Linux release archive's `k` | exists (built only by the workflows) |
| `scripts/install.sh`, `scripts/install.ps1` | the installers, served at the docs site's root by the Pages deploy: `curl -fsSL https://slavkobojanic.github.io/kobra/install.sh \| sh` (`irm https://slavkobojanic.github.io/kobra/install.ps1 \| iex` on Windows); they download the public release files with no token, check `SHA256SUMS` and install the toolchain and its book; tested by `crates/k_cli/tests/install_script.rs` against a fake release, docs site and GitHub API on loopback | exist |
| `Cargo.toml` `[workspace.metadata.project]` | **the** name and namespace: `Kobra`, the repository, the docs site (for the day the repository is public), the image, the extension id; read by `release prepare` and `release attach-local`, checked against the rest by `version.rs` | exists |
| `LICENSE-MIT`, `LICENSE-APACHE` | the licence (MIT OR Apache-2.0), packed into every archive | exist |
| `tests/compat/locks/`, `tests/compat/wire/` | golden lockfiles per format and recorded traffic per minor; checked by `crates/k_server/tests/compat/` | exists (format 1 of every lockfile; the 0.1 transcripts) |
| `crates/k_cli/tests/version.rs` | every version string agrees; `k --version` format | exists (the commit and date in `--version` come with step 1's `build.rs`, in round 3) |
| `k.lock` (`toolchain = "…"`) in every example app and workspace root | the toolchain that last built it; `version.rs` checks it names the version | exists |
| the test projects' `k.toml` and `k.lock` (`tests/e2e/`, `crates/*/tests/`) | the minor and version the tests run them with; moved by `release prepare` and `start-next`, checked by `version.rs` (as are the manifests the tests write, through `k_requirement!()`) | exists |
| `crates/xtask/tests/checkout.rs` | no test fixture is git-ignored, so a fresh checkout (CI's) runs what a working copy runs | exists |
| `crates/k_cli/tests/toolchain.rs` | requirement, delegation, `K_TOOLCHAIN`, `k.lock` updates against fake installed toolchains; `k toolchain` and `k self update` against a fake release (`tests/support/release.rs`) on loopback | exists |
| `docs/book/src/apps/upgrading.md`, `reference/changelog.md` | the upgrade guide (procedure, then one generated section per minor, cumulative) and the changelog page ([design 49](../design/49-documentation.md)) | exist (their generated parts empty until 0.1.0) |
| `docs/book/check/audit.toml` | the page-audit ledger ([documentation.md §5](documentation.md#5-the-page-audit)) | planned |
| `docs/book/check/released-pages.txt` | every page path of the last release, for the redirect test | planned |

`cargo xtask` (an alias in `.cargo/config.toml`) runs `crates/xtask`, a workspace member that is never
shipped; it uses only crates already listed in
[architecture.md §15](architecture.md#15-dependencies), plus whatever the signing step needs, which is
added there when it is chosen.

---

## 3. For contributors: changelog fragments

Every pull request with a user-visible change adds one file to `changes/`, named after the change:

```markdown
---
kind: stdlib
design: 37
book: apps/charts.md
---
`BarChart` draws grouped bars: `BarChart(series:)` takes several series.
```

- **Kinds**, in the order the changelog lists them: `breaking`, `deprecation`, `language`, `stdlib`,
  `runtime`, `tooling`, `preview`, `fix`, `docs`, `internal`.
- **`breaking` and `deprecation` need `migration:`**: the diagnostic code whose machine-applicable fix
  migrates (`migration: K3108`), or `manual`, in which case the same pull request adds the section to
  `docs/book/src/apps/upgrading.md`.
- **Write it for users:** one or two sentences in the present tense, saying what they can now do or must
  change. It is published as written.
- **Nothing user-visible?** Add a `kind: internal` fragment with a one-line reason. CI requires a
  fragment in every pull request that touches `crates/`, `editors/` or `docs/book/`; internal ones are
  dropped at release.
- **`book:`** is required on the user-visible kinds (`breaking`, `deprecation`, `language`, `stdlib`,
  `runtime`, `tooling`, `preview`): the book pages this pull request changes, or `none: <reason>`.
  `cargo xtask docs-gate origin/main` checks the listed pages are in the diff
  ([design 49](../design/49-documentation.md#docs-ownership-in-review)); run it before pushing.
- `cargo test -p xtask` checks every fragment (front matter, known kind, migration present and one
  `k upgrade` applies, `book:` present where required and every page in it exists, a manual migration's
  anchor in the upgrade guide); `cargo xtask fragments` lists them.

---

## 4. For contributors: breaking and format changes

### A breaking language or standard library change

Before 1.0 this takes two minors; after 1.0 it needs an edition (design 48, "Editions").

1. **Minor N: deprecate.** Add `@Deprecated("…", since: "0.N", replacement: …)` to the declaration, or a
   deprecation warning for the syntax form, with a machine-applicable fix where one is possible. Add a
   corpus case for the warning and for the fix's result (`tests/corpus/sema/…`, blessed). Fragment:
   `kind: deprecation`, `migration: <code>`.
2. **Minor N+1 at the earliest: remove.** Remove the declaration or form, and leave a **tombstone**: the
   parser or checker still recognises the old form and reports it as an error carrying the same fix.
   Fragment: `kind: breaking`, same `migration:`.
3. **Minor N+2: drop the tombstone.** Fragment `kind: internal`.
4. In every step, the book is updated in the same pull request; its snippets would fail otherwise.

**Patch releases take none of these.** A change that adds an error or a warning waits for the next minor,
unless it is a soundness or security fix (then: fragment `kind: breaking`, and the reason in it).

### A lockfile format change

1. Bump the file's number in `k_sema::lockformat::FORMATS` (and `k_db::lock::FORMAT` for
   `schema.lock`); keep the reader for every older format.
2. Add the new format's golden file as `tests/compat/locks/<lockfile>/<N>/<lockfile>` (for the program in
   `tests/compat/locks/project/`) and keep the old ones, and add the conversion from `N−1` to
   `k_sema::lockformat::STEPS` (none when only the readers changed); `cargo test -p k_server --test it --
   compat::locks::` reads each, and checks that `k upgrade`'s conversion rewrites each to the golden of `N`.
3. A toolchain writes the format it read; only `k upgrade` moves a project to the new format.
4. Fragment `kind: tooling` naming the format change.

### A wire or stored-format change

Anything that two running versions exchange: rpc encoding, error details, stream framing, the service
token, the session cookie, `k_job` payloads, the `k-data` payload, the runtime's own tables.

1. **Minor N: read both, write the old.** The new form is accepted everywhere it can arrive.
2. **Minor N+1: write the new.** The release notes' highlights say so: operators must not skip a minor in
   one rolling deploy.
3. **Minor N+2 at the earliest: stop reading the old.**
4. Each step re-records `tests/compat/wire/<minor>/` (`K_BLESS=1 cargo test -p k_server --test it --
   compat::wire::`, which records only the current minor) and keeps the previous minor's recordings,
   which the current build must still accept.
5. A runtime-owned table follows expand/contract the same way, as a toolchain migration that `dist/app`
   applies under the migration lock.

---

## 5. Branches, tags and versions

- **`main`** is always the next minor. Its version is the next minor's `X.Y.0` with no pre-release
  suffix; nightly builds add `-nightly.<date>` at build time, not in `Cargo.toml`.
- **`release/X.Y`** is cut from main for each minor; `X.Y.*` and docs fixes for `/X.Y/` are made there,
  by cherry-picking fixes that landed on main first (fix on main, then backport, never the reverse).
- **Tags** are `vX.Y.Z` and `vX.Y.Z-rc.N`, annotated, on the release branch (or main, for a release
  made from it). **The release workflow creates them**, on the commit it gated
  (`gh workflow run release.yml -f version=…`, §9); nobody pushes a release tag by hand. **A tag is
  never moved or reused**; a bad release is yanked and fixed by the next patch (§11).
- **Only the release pipeline publishes.** Nothing is uploaded from a maintainer's machine.

---

## 6. Cutting a minor release

Timeline for `0.6.0`, with T the planned release day. In short, every release is:

```sh
cargo xtask release prepare 0.6.0                           # then commit it to the branch and push
gh workflow run release.yml --ref release/0.6 -f version=0.6.0
gh run watch                                                # pick the run; ends when the draft exists
git fetch origin tag v0.6.0 && git switch --detach v0.6.0   # on the owner's Mac
cargo xtask release attach-local v0.6.0
gh workflow run publish.yml -f version=0.6.0
```

`--ref` is the branch whose head is released (and whose `release.yml` runs); without it, main. A
release made from main (the first ones, before release branches are cut) leaves it out.

### T−7: branch cut and release candidate

1. **Check main is green:** a nightly of the commit you will cut passed (fuzz, long property runs,
   performance budgets included; start one with `gh workflow run nightly.yml -f publish=false` if
   none has run on it). If it didn't, the cut waits; the train doesn't ship red.
2. **Cut the branch:**
   ```sh
   git switch main && git pull
   git switch -c release/0.6
   ```
3. **Prepare**:
   ```sh
   cargo xtask release prepare 0.6.0-rc.1
   ```
   It sets the version everywhere ([§2](#2-files-the-release-process-owns), the test projects
   included), compiles the fragments
   into a draft `CHANGELOG.md` section, the changelog page and the upgrade guide's new section (it does
   not delete fragments for a release candidate), updates `k = "0.6"` in the examples and the `k new`
   skeleton, runs `cargo test -p k_cli --test it -- version::` and prints what it changed. It will also
   print the book pages whose audit-ledger entry predates the previous minor's cut and whose design topic
   changed since *(planned: that needs a previous release tag, so it comes with the first minor after
   0.1)*: re-audit those before tagging the candidate.
4. **Review the draft notes:** read every fragment in the context of the others, fix wording in the
   fragments themselves, and write `changes/_highlights.md` (three to six bullets; anything operators
   must know first, such as a new wire encoding being written). Write them on main before the cut when
   you can (0.1's are there already), so the candidate's section has them and `prepare` needs no
   second run; re-running it for the same candidate replaces that candidate's section.
5. **Run the local gate:**
   ```sh
   cargo xtask release check
   ```
   which is `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
   `k fmt --check examples`, `cargo test --workspace` (the fragment checks and version consistency
   included), the compat suite, the `K_REQUIRE_CHROME=1` real-browser suites and `cargo xtask docs build
   --version next` (the book with no mdBook warning, as the versioned site); `--list` prints it. It is
   the same list CI runs, so a failure here saves a pipeline run. Run it in a **fresh clone** of the
   branch, not your working copy: git-ignored files there (a fuzz corpus, an `.env`) hide what CI
   will lack, as the `0.1.0-rc.1` dry run found.
6. **Commit, push and start the release** (no tag: the workflow makes it):
   ```sh
   git commit -am "Release 0.6.0-rc.1"
   git push origin release/0.6
   gh workflow run release.yml --ref release/0.6 -f version=0.6.0-rc.1
   gh run watch
   ```
   Its first job, `check` (`cargo xtask ci check-version`), refuses a version that isn't the branch's,
   has no `CHANGELOG.md` section or is tagged already, and prints the command that fixes it (usually
   `cargo xtask release prepare <version>`).
7. **Bump main** to the next minor in its own pull request: `cargo xtask release start-next 0.7.0`
   sets `0.7.0` and leaves the fragments on main untouched.
8. The release workflow (§9) runs the gate on the branch's head, builds the candidate's Linux and
   Windows archives and, when everything passed, **tags that commit `v0.6.0-rc.1`** (annotated, pushed
   by the workflow) and makes a **draft pre-release**: archives, the book, checksums, signatures, the
   index entry and the whole index (its version carries `-rc.1`, so `k toolchain` installs it only when
   named exactly), the extension `.vsix`, the JetBrains plugin `kobra-intellij-0.6.0-rc.1.zip`, the
   image tag `0.6.0-rc.1-<arch>` (with `K_PUBLISH_ENABLED`). `-f draft=false` publishes it at once instead, before the macOS archive is
   there; leave it a draft unless there is no Mac to attach from.
9. **Attach the macOS archive** from the owner's Mac (no CI job runs on macOS: decided while the repository was
   private, where a macOS minute is billed at ten Linux minutes, and kept since), in a clean checkout of the tag, signed
   in with `gh auth login` as an account that can write releases:
   ```sh
   git fetch origin tag v0.6.0-rc.1
   git switch --detach v0.6.0-rc.1
   cargo test --workspace                       # the gate's macOS column, by hand
   K_REQUIRE_CHROME=1 cargo test -p k_cli --test it -- chrome:: dist_browser::
   cargo xtask release attach-local v0.6.0-rc.1
   ```
   `attach-local` builds `aarch64-apple-darwin` with `--locked`, smoke-tests it (`dist smoke`; skip
   with `--no-smoke` only when it already passed), packs `k-<version>-aarch64-apple-darwin.tar.gz`
   with the licences, downloads the draft's `SHA256SUMS`, `index-entry.json` and `index.json` with
   `gh release download`, merges the archive's line and entry into them (replacing an earlier one of the
   same target, so it can be rerun), and uploads all four with `gh release upload --clobber`.
   `--archive <file>` attaches an archive built already; `--target` builds another target this Mac
   can build and run. `x86_64-apple-darwin` isn't offered (it needs V8's x86_64 library, an extra
   download, and Rosetta to smoke-test). The macOS archive has no Sigstore bundle or provenance, only
   its `SHA256SUMS` digest. Then publish the draft (a candidate needs no review beyond the jobs passing):
   `gh workflow run publish.yml -f version=0.6.0-rc.1` (§9, "`publish.yml`").
10. Announce the candidate to the collaborators with its install line (`k toolchain install
   0.6.0-rc.1`, or the install script with `--version 0.6.0-rc.1`: without it the script installs the
   newest stable release, and the newest candidate while there is none), `K_TOOLCHAIN=0.6.0-rc.1` for
   one command, and ask that `k upgrade --to 0.6.0-rc.1` be tried on real projects.

### T−7 to T: soak

- Fixes land on main first, then are cherry-picked to `release/0.6` with their fragments.
- Another candidate (`rc.2`) only if a fix changed behaviour beyond the bug.
- Re-run `scripts/bench-check.sh` on the performance runner for the branch head; record the numbers in
  the release checklist issue.

### T: the release

1. ```sh
   git switch release/0.6 && git pull
   cargo xtask release prepare 0.6.0
   ```
   This time the fragments are compiled and **deleted**, `CHANGELOG.md` gets its final section with the
   date, and `_highlights.md` is emptied.
2. `cargo xtask release check`, then commit and push the branch, and start the release:
   `gh workflow run release.yml --ref release/0.6 -f version=0.6.0`, then `gh run watch`.
3. The release workflow runs the full gate on the branch's head on Linux and Windows and builds their
   artifacts and the book; only when all of it passed does it tag that commit `v0.6.0` and push the tag.
   It creates a **draft** release; nothing is visible yet.
4. **Attach the macOS archive** from the owner's Mac, as in step 9 of the candidate:
   `cargo xtask release attach-local v0.6.0` in a checkout of the tag (`git fetch origin tag v0.6.0`
   first), after the macOS tests.
5. **The release manager reviews the draft**: every CI target's smoke test passed and the macOS one
   passed locally, `SHA256SUMS` lists every archive and `docs-0.6.0.tar.gz`, `index.json` lists every
   target, the signature verification job passed, the notes render, and the book (`k docs` after
   installing the draft's archive by hand, or the `site` artifact) looks right.
6. **Publish the draft:** `gh workflow run publish.yml -f version=0.6.0`. It refuses a release without
   the macOS archive (`-f macos=false` overrides), publishes the draft, rebuilds its `index.json` from
   every published release, makes the image tags `0.6.0`, `0.6`, `latest` (with `K_PUBLISH_ENABLED`),
   and, once the repository is public, deploys `/0.6/` with `/` and `/latest/` pointing at it. *(Planned:
   the extension to its marketplaces and package-manager updates, once chosen.)* Publishing the draft
   on the release page instead runs none of the rest; run `publish.yml` afterwards, which leaves a
   published release as it is and does the rest.
7. **Merge the release back:** a pull request from `release/0.6` to main carrying `CHANGELOG.md` and the
   book's new changelog and upgrade-guide sections, and deleting on main the fragments that shipped
   (`cargo xtask release sync-main` *(planned)* computes the list).

---

## 7. Cutting a patch release

1. The fix is on main, with its fragment (`kind: fix`, or `breaking` for a soundness or security fix).
2. Cherry-pick it to `release/0.6`, fragment included.
3. `cargo xtask release prepare 0.6.1`, `cargo xtask release check`, commit, push, and
   `gh workflow run release.yml --ref release/0.6 -f version=0.6.1`.
4. Steps 3–7 of "T: the release" above (the macOS archive included). The release's book replaces the
   minor's, and once the repository is public the docs deploy replaces `/0.6/`.

`cargo xtask release prepare` refuses a patch whose fragments include `deprecation`, `language`,
`stdlib` or `preview` kinds, and warns on `breaking`, so a minor's changes can't slip into a patch.

A **docs-only fix** to a released minor needs no tag: merge it into `release/0.6`; once the repository
is public, `gh workflow run publish.yml -f version=<the minor's newest release>` redeploys the site
*(planned: a deploy of `/0.6/` from the branch's head rather than from the tag)*.

---

## 8. Nightly

- **Only on request** (design 48, "CI on request"): `gh workflow run nightly.yml` builds main's head
  (`--ref <branch>` for another branch), and `-f publish=false` runs the checks without publishing.
  There is no schedule: a nightly is a build of main someone asked for, before a branch cut (§6 step 1)
  or to give collaborators main's latest. The name stays "nightly", as do the version
  (`X.Y.Z-nightly.<date>`), the channel and `k toolchain install nightly`.
- **One a day**: the version and the tag carry the date, so a second published nightly on the same day
  fails at `gh release create`; run it with `-f publish=false`, or wait for the next day.
- Runs the nightly column of the gate: every target's tests, fuzzing for a fixed time per target, long
  property runs, performance budgets on the dedicated runner, the smoke tests and the image build.
- **On green**, unless `publish=false`, publishes `0.7.0-nightly.<date>` (the Linux and Windows archives, main's book,
  `SHA256SUMS`, its index entry and the whole index, as a GitHub pre-release). Nightlies have no macOS
  archive. *(Planned: the image tag `nightly` and the extension as a pre-release.)*
- **On red**, publishes nothing, opens or updates one issue with the failing jobs, and blocks the next
  branch cut until a later nightly is green.
- New fuzz crashes are minimised and committed under `fuzz/artifacts/` with a regression case in the
  corpus ([testing.md §6–7](testing.md#6-fuzzing)) before the issue is closed.

---

## 9. The pipeline

GitHub Actions (owner decision, 2026-11-09; the jobs translate directly to another CI system). Every
job runs `cargo` commands that also work locally; YAML holds no logic that `cargo xtask` doesn't, and
the few steps that need `gh` or `cosign` are scripts in `scripts/ci/`. The shared setup (Rust, mdBook
0.5.4, the caches) is the local action `.github/actions/setup`; assembling the whole docs site is
`.github/actions/site`.

### Running CI

**CI runs only on request** (owner decision, 2026-11-23; design 48, "CI on request"). Every workflow's
one trigger is `workflow_dispatch`: nothing runs on a pull request, a push, a tag, a published release
or a schedule. A run is started from the repository's Actions page ("Run workflow") or with the GitHub
CLI:

| Command | Runs |
|---|---|
| `gh workflow run ci.yml --ref <branch>` | the change checks on the branch's head, against `origin/main` (`-f base=<rev>` for another base) |
| `gh workflow run nightly.yml` | a nightly of main (`--ref` for another branch); `-f publish=false` runs it without publishing |
| `gh workflow run release.yml [--ref release/0.6] -f version=0.6.0` | a release of the branch's head: the gate, the artifacts, the tag `v0.6.0`, a draft release (`-f draft=false` publishes it at once) |
| `gh workflow run publish.yml -f version=0.6.0` | publishing that release once the macOS archive is attached: the draft published, its index refreshed, the image tags, the docs deploy |
| `gh run watch`, `gh run list --workflow release.yml` | follow a run, list them |

`--ref` names the branch whose head is checked out **and** whose copy of the workflow runs, so the
workflow and the code it checks are always one commit. Since starting a workflow is the switch, the
old `K_CI_ENABLED` variable is gone. What remains are repository variables (Settings → Secrets and
variables → Actions → Variables) for parts that cost something or can't run yet:

| Variable | Turns on |
|---|---|
| `K_PAGES_ENABLED` = `true` | the Pages deploys (`pages` in `ci.yml` and `publish.yml`); first set Settings → Pages → Source to "GitHub Actions". **Set (2026-11-23)**, with Pages' source set, since the repository went public (design 48, "Public distribution") |
| `K_PUBLISH_ENABLED` = `true` | pushing the toolchain images to GHCR (`image` in `release.yml`, `publish-image` in `publish.yml`): the images are built and checked on every release but pushed only with it (a switch made while a private repository's packages counted against the plan's storage, kept as the owner's choice). Releases and nightlies are published by being started, without it |
| `K_PERF_RUNNER` = a runner label | the performance budgets (`perf`, `gate-perf`) on that self-hosted runner; unset, they are skipped |

Removing a variable (or setting it to anything but `true`) switches its jobs off again. With no
pull-request trigger, nothing reports checks on a pull request by itself: a reviewer starts `ci.yml` on
its branch (the run's checks show on the branch's head commit), and branch protection's required
checks stay an owner decision (design 48). `crates/xtask/tests/workflows.rs` fails if a workflow has
any trigger but `workflow_dispatch`, if anything names `K_CI_ENABLED`, if a script pastes a
`${{ inputs.… }}` in instead of reading it from `env:`, if a Pages deploy or an image push lacks its
switch, if `release.yml` lacks its `version` input or tags in any job but `release` or before the gate
and the builds passed, if an action isn't pinned to a release, if a command names a script, crate, test
module, fuzz target, Dockerfile or `cargo xtask` command that doesn't exist, or if a row of design 48's
gate isn't run where its columns say.

### `ci.yml`: the change checks, on request

| Job | Runner | Runs |
|---|---|---|
| `lint` | Linux x86_64 | `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `k fmt --check examples` (a compiler-only `k`: `cargo run -p k_cli --no-default-features`) |
| `test-linux` | Linux x86_64, with a Postgres service and Chrome | `cargo test --workspace` with `K_TEST_POSTGRES_URL` and `K_REQUIRE_CHROME=1` |
| `book` | Linux x86_64 | `cargo xtask docs build --version next` (the book, any mdBook warning or error fails), the link check (`book_links::`; the other freshness checks are inside `cargo test`); when `docs/book/` or `crates/xtask/` changed since the `base` input (`cargo xtask ci changed`), the built site uploaded as an artifact (the change's preview until the owner picks a host with previews) |
| `fragments` | Linux x86_64 | `cargo test -p xtask`; `cargo xtask docs-gate <base>` (a fragment when `crates/`, `editors/` or `docs/book/` changed since the base, and its pages in the diff; nothing to check on main itself) |
| `vscode` | Linux x86_64, Node 22 | `node --test` in `editors/vscode`: the preview client and panel against a fake `k preview` and a stub of the `vscode` module (no npm install; the test against a real `k` skips, as the job builds none) |
| `intellij` | Linux x86_64, Java 25 | `./gradlew test buildPlugin verifyPlugin` in `editors/intellij`: the JetBrains plugin's tests in a headless IDE, the plugin verifier, and the zip uploaded as an artifact |
| `install-scripts` | Linux x86_64; only when `scripts/install.*` or their tests changed since the base | `install_script::`: each installer against a fake release, docs site and GitHub API on loopback |
| `pages` | Linux x86_64; a run of main, after the jobs above; needs `K_PAGES_ENABLED` (set) | the whole site (the newest release from its tag, `/next/` from main, `install.sh`, `install.ps1` and the index at its root) deployed to GitHub Pages |

The `base` input (default `origin/main`) is what a pull request's base was: the commit the change is
compared with. **Linux only**, as decided while the repository was private, where GitHub bills a macOS minute at
ten Linux minutes and a Windows minute at two, and kept since it is public: `ci.yml` runs on Linux, Windows is tested by the release's
and the nightly's gate, and macOS on the owner's Mac before each release (§6). `workflows.rs` fails
when a workflow names a macOS runner or target.

Caching: the cargo registry, `~/.cargo/bin` (mdBook, `cargo-fuzz`) and `target/`, keyed by `Cargo.lock`,
the runner and the job; `target/` holds the V8 library that `deno_core`'s build downloads.

### `nightly.yml`: a nightly, on request

`gh workflow run nightly.yml` (§8); there is no schedule. The `publish` input (default `true`) decides
whether a green run publishes.

| Job | Runner | Runs |
|---|---|---|
| `test` | one per CI target: Linux x86_64 and arm64, Windows x86_64 | `cargo test --workspace` |
| `services` | Linux x86_64 with Postgres | `cargo test -p k_db -p k_server` on Postgres, `chrome:: dist_browser::` with `K_REQUIRE_CHROME=1`, `install_script::` |
| `lint-and-docs` | Linux x86_64 | formatting, lints, `k fmt --check examples`, `cargo xtask docs build --version next` (uploaded as the nightly's book), `compat::` |
| `properties` | Linux x86_64 | the property suites with raised `PROPTEST_CASES` ([testing.md §2](testing.md#2-running-tests)) |
| `fuzz` | Linux x86_64, one per target | `fuzz/seed-corpus.sh`, then `cargo +nightly fuzz run <target> -- -max_total_time=600`; a crash is uploaded |
| `perf` | `K_PERF_RUNNER` | `scripts/bench-check.sh` |
| `build` | one per CI target; Linux in `rust:bullseye` (glibc 2.31) | `cargo xtask release nightly` (the version `X.Y.Z-nightly.<date>`, in the checkout only), `cargo build --release -p k_cli --locked`, `cargo xtask dist archive`, `cargo xtask dist smoke` |
| `image` | Linux x86_64 and arm64 | build `docker/toolchain.Dockerfile` from the archive's `k`, then `examples/shop/Dockerfile` with it |
| `publish-nightly` | needs every job above but `perf`; skipped with `-f publish=false` | `cargo xtask dist docs` (main's book as `docs-<version>.tar.gz`), `SHA256SUMS`, the index entry, the whole index (`scripts/ci/release-index.sh`), and a GitHub pre-release `v<version>` with them and the archives |
| `report` | when anything failed | opens or comments on the one `nightly-failure` issue (`scripts/ci/nightly-issue.sh`) |

*(Planned: the nightly's image tag `nightly` and the extension pre-release; marketplaces wait for the
name.)*

### `release.yml`: a release, started with its version

`gh workflow run release.yml [--ref release/0.6] -f version=0.6.0`, with the inputs `version`
(required: `0.6.0`, `0.6.0-rc.1`; a leading `v` is accepted) and `draft` (default `true`). The version
comes from the input, never from the name of a pushed ref, and every job checks out the dispatched
commit, so the gate, the artifacts and the tag are of one commit.

0. **`check`**: `cargo xtask ci check-version <version>` in a checkout with the tags. It refuses a
   version that isn't one, a nightly, a version other than the branch's `Cargo.toml` (it names
   `cargo xtask release prepare <version>` to run, commit and push first), one with no `CHANGELOG.md`
   section, and one whose tag exists (a tag is never moved or reused, §5). The workflow doesn't run
   `prepare` itself: what is released is a commit someone reviewed, and CI commits nothing. Its
   outputs (`version`, `tag`, `minor`, `prerelease`) are what every later job reads, so every job
   `needs: check`.
1. **The gate** on the commit: `gate-test` (`cargo test --workspace` and `install_script::` on Linux
   x86_64 and arm64 and Windows x86_64), `gate-services` (Postgres, real Chrome on Linux), `gate-checks`
   (formatting, lints, `k fmt --check examples`, `cargo xtask fragments --none-left` for a release and
   `cargo xtask fragments` for a candidate, which keeps its fragments, `version::`, `cargo test -p xtask`,
   `compat::`, `cargo xtask docs build --version <version>`), `gate-properties` (a shorter run),
   `gate-fuzz` (each target over its seeds, no new inputs: `fuzz/seed-corpus.sh` fills the
   git-ignored `fuzz/corpus` from committed files, then `cargo +nightly fuzz run <target>
   fuzz/corpus/<target> -- -runs=0`), `gate-perf` (on `K_PERF_RUNNER`; skipped without
   one, and then run by hand, §6). The macOS column runs on the owner's Mac (§6).
2. **`build`**: per CI target (Linux x86_64 and arm64, Windows x86_64) on its native runner,
   `--locked`; Linux in `rust:bullseye` for the glibc 2.31 baseline; `cargo xtask dist archive` names it
   `k-<version>-<target>.tar.gz` (`.zip` on Windows), with the README and both licences. The macOS
   archive is attached later from the owner's Mac (`cargo xtask release attach-local`, §6).
3. **`smoke`**: on a fresh runner of the target: unpack the archive, then `cargo xtask dist smoke`
   (`k new hello`, `k check`, `k test`, `k build`, `dist/app serve`, `dist/app probe …/_k/health`).
4. **`checksums`**: `cargo xtask dist checksums` (`SHA256SUMS`, over the archives and the book) and
   `cargo xtask dist index` (`index-entry.json`, this release's entry of the toolchain index, with the
   book as `docs`).
5. **`sign`**: a Sigstore bundle per archive (`scripts/ci/sign.sh`, keyless, `id-token: write`) and
   build provenance (`actions/attest-build-provenance`), then **`verify`**, which downloads the
   artifacts as a user would and checks the digests and the signatures against the workflow's
   identity at the ref it was started on (`…/release.yml@refs/heads/release/0.6`;
   `scripts/ci/verify.sh`).
6. **`extension`**: `npm ci`, `node --test` (the preview panel's tests) and `npx @vscode/vsce package` in `editors/vscode` (npm is used here and only
   here, [architecture.md §15](architecture.md#15-dependencies)): `kobra-<version>.vsix`. Beside it,
   **`intellij`**: `./gradlew test buildPlugin verifyPlugin` in `editors/intellij` (Java 25; Gradle,
   the IntelliJ Platform Gradle Plugin and the IDE from Gradle's and JetBrains' repositories): the
   plugin's tests in a headless IDE, the plugin verifier against the IDE it targets, and the plugin as
   `kobra-intellij-<version>.zip`.
7. **`image`**: the toolchain image per architecture, checked by building `examples/shop` with it; pushed
   as `<version>-amd64` and `<version>-arm64` only with `K_PUBLISH_ENABLED`.
8. **`docs`**: `cargo xtask docs build --version <version>`, the book with the version substituted,
   uploaded as an artifact for review, and `cargo xtask dist docs target/site/<minor>`, the book as
   `docs-<version>.tar.gz`, which the release attaches and the installers unpack for `k docs`.
9. **`release`**, only when every job above passed (a skipped `gate-perf` doesn't block it, a failed
   one does), and the only job that may write the repository (`contents: write`):
   `scripts/ci/release-index.sh` (`index.json`, every published release's entry merged with this
   one's), `cargo xtask ci notes <tag> --links <repository>/blob/<tag>` (the notes from
   `CHANGELOG.md`, with the book links made absolute: a release page doesn't resolve relative ones),
   then **`scripts/ci/tag.sh`**, which creates the annotated tag `v<version>` ("Kobra <version>") on
   the dispatched commit and pushes it (a re-run of the job finds it on that commit and goes on; a tag
   on another commit is refused), then `gh release create --verify-tag`: a draft (unless
   `draft=false`), a pre-release for a version with a pre-release part, with every archive and its
   Sigstore bundle, the book, `SHA256SUMS`, `index-entry.json`, `index.json`, the `.vsix` and
   `kobra-intellij-<version>.zip`. That is everything `attach-local` merges the macOS archive into.
   Pushing with the workflow's token starts no other workflow, and none would start on a tag anyway.

### `publish.yml`: publishing a release, on request

`gh workflow run publish.yml -f version=0.6.0` once the macOS archive is attached (inputs `version`,
required, and `macos`, default `true`). Publishing used to be the `release: published` jobs of
`release.yml`; with no event triggers they are their own workflow, started like the others.

1. **`check`**: `cargo xtask ci check-version <version> --tagged`: a version whose tag exists.
2. **`publish`** (`contents: write`): `scripts/ci/publish-release.sh` refuses a release without the
   `aarch64-apple-darwin` archive unless `-f macos=false`, and publishes the draft (`gh release edit
   --draft=false`; a published one is left as it is); then the release's `index.json` rebuilt from
   every published release's `index-entry.json`, the macOS entry `attach-local` merged included
   (`scripts/ci/release-index.sh`), uploaded over the draft's.
3. **`publish-image`** (`K_PUBLISH_ENABLED`): the multi-arch `<version>` tag, and for a release, not a
   candidate, `X.Y` and `latest`.
4. **`pages`** (`K_PAGES_ENABLED`, once the repository is public): the site rebuilt whole by
   `.github/actions/site`: `cargo xtask docs build --version latest` (the newest stable release, from
   its tag, with `/` and `/latest/` pointing at it), `--version next --from origin/main`,
   `versions.json`, the install scripts with the release host filled in, and `index.json` merged from
   every published release's `index-entry.json` (`scripts/ci/release-index.sh`, `cargo xtask dist
   merge-index`). A candidate's publication updates the index but not the docs.

Each job is idempotent, so a failed one is re-run on its own, and so is the whole workflow.
*(Planned: the marketplace and package-manager jobs, once the owner enables them; yanking through the
index, with `cargo xtask release yank`.)*

**Permissions:** the workflow token is read-only by default; only `sign` (`id-token`, `attestations`),
`release` and `publish` (`contents`; in `release.yml`, `release` alone), `image` and `publish-image`
(`packages`), `pages` (`pages`, `id-token`), `publish-nightly` (`contents`) and `report` (`issues`) get
write access, which `workflows.rs` checks.
Marketplace and package-manager tokens, once chosen, are environment secrets scoped to a `release`
environment that requires a maintainer's approval.

---

## 10. After publishing

1. **Install as a user would**, on a machine without Kobra and without `gh` or a token:
   `curl -fsSL https://slavkobojanic.github.io/kobra/install.sh | sh`, then `k --version`, `k docs --path`, `k new hello && cd hello && k dev`.
2. **Upgrade as a user would:** on a project pinned to the previous minor, `k toolchain install`,
   `k upgrade`, `k check`. Note anything `k upgrade` couldn't fix that the upgrade guide doesn't cover,
   and fix the guide on the release branch.
3. **Check the docs:** `k docs` opens the new release's book. On the site, `/` lands on
   the new minor, the version switcher lists it, `/next/` still builds from main, an old minor's URL
   redirects (before 1.0).
4. **Close the release checklist issue** with the performance numbers and links to the artifacts.

---

## 11. When a release is bad

- **Never delete or re-tag.** Projects' `k.lock` files and other people's caches name the version.
- **Yank:** mark the version `yanked` in the index (`cargo xtask release yank 0.6.1` *(planned)* opens
  the pull request; the newest release's `index.json` is replaced with the marked one). `k toolchain install` then skips it unless it is
  named exactly, and `k toolchain list` and the delegation diagnostic say it is yanked, with the reason
  (as of the index each machine last fetched: nothing is fetched to say so).
  Its archives stay downloadable, so pinned builds still reproduce.
- **Fix forward:** the next patch, through §7, with a fragment explaining the problem and the fix.
- **A wrong book:** rebuild it from the release branch (`cargo xtask docs build`, `cargo xtask dist
  docs`) and replace the release's `docs-<version>.tar.gz` and its `SHA256SUMS` line; once the repository
  is public, rerun the docs job. The docs are a pure function of the branch.
- **A security problem:** reported privately (the address is in `SECURITY.md` *(planned)*); fixed in a
  private fork; patch releases are prepared for the newest minor and, when it is affected, the previous
  one; published together, with an advisory. The fragment is written only in the private fork.

---

## 12. Implementation plan

Each step starts with failing tests, in the style of [testing.md](testing.md). No step publishes anything
or needs an account; step 13 writes CI files that run only when the owner starts them
([§9](#running-ci)).

**Scheduling:** these steps are run in the rounds of the combined plan in
[design 49](../design/49-documentation.md#implementation-plan), together with the documentation work:
step 1 in round 1; steps 2, 3, 5, 6, 7, 9, 10 in round 3; steps 4, 8, 11, 12, 13, 14 in round 4; step 15
in round 5.

1. **Version consistency.** Test first: `crates/k_cli/tests/version.rs` asserts that `k --version`
   prints `k <version> (<commit> <date>)` and that `Cargo.toml`, `editors/vscode/package.json`,
   `editors/intellij/gradle.properties`, the
   examples' `k = "…"`, the image tags in `docs/design/38-deployment.md` and the examples' Dockerfiles,
   and the book's install page agree. Then: embed the commit and date at build time (a `build.rs` in
   `k_cli`, falling back to `unknown` outside git), and fix any disagreement.
2. **Toolchain requirement.** Tests in `k_sema/tests/workspaces.rs` and a new
   `crates/k_cli/tests/toolchain.rs`: `k = "0.5"` accepts 0.5.x and rejects 0.6 and 0.4 (and, for
   `1.4`, accepts 1.9); `=0.5.2` is exact; a nightly of 0.7 satisfies `0.7` only; a member incompatible
   with `[workspace] k` is an error; a malformed requirement is `K8101`. Then: a small semver requirement
   parser in `k_sema::workspace` (no new crate) and the diagnostic for a running toolchain that doesn't
   match.
   *As built (2026-11-12):* `k_sema::workspace::{Version, Requirement}` (`workspace/requirement.rs`),
   tested in `k_sema/tests/workspaces.rs` (`requirements::`). A requirement is one to three numbers with
   an optional `=`; a pre-release is written only exactly (`=0.6.0-rc.1`) and satisfies a caret
   requirement when its release does. `Workspace::requirements()` lists the root's `[workspace] k` and
   every member's `[package] k`; a member may omit `k` when the root has one, and one that intersects
   nothing the root allows is the new `K8107 conflicting-toolchain`. The mismatch diagnostic is
   `K8201 toolchain-mismatch`, reported by the driver (step 3), not by the analysis. Corpus case:
   `tests/corpus/sema/workspaces/conflicting-toolchain/`.
3. **`k.lock` toolchain, delegation and `K_TOOLCHAIN`.** Tests with a fake `~/.k/toolchains/` (a
   directory of scripts that print their arguments): delegation runs the locked version with identical
   arguments and streams; `K_TOOLCHAIN_DELEGATED` stops recursion; a missing toolchain exits 2 with the
   install command in human and JSON output; a patch difference runs and `k build` updates the lock;
   `k lsp` delegates too. Then the implementation in `k_cli`.
   *As built (2026-11-12):* `crates/k_cli/src/toolchain.rs`, run in `main` before and after argument
   parsing; tests in `crates/k_cli/tests/toolchain.rs`. Toolchains are `$K_HOME/toolchains/<version>/k`
   (`K_HOME` defaults to `~/.k`, so tests need no fake home directory); on Unix the running `k` replaces
   itself with the chosen one (`exec`), elsewhere it waits for it and returns its status. Decisions made
   while building, recorded in design 48: `K_TOOLCHAIN` applies to every command (`--version`
   included) and wins over the requirement; without a lock only releases are chosen, never a
   pre-release; a lock the requirement no longer allows is ignored; an unreadable `k.lock` stops the
   command with `K8002`; `k build` writes `k.lock` in an app or at the workspace root (never in a
   member or a library), only when the running toolchain satisfies the requirement. `k new` still
   writes no lock: the first `k build` does. Every example app and workspace root now commits its
   `k.lock`, and the shop and stack roots set `[workspace] k`.
4. **`k toolchain` and `k self update`.** Tests against an index and archives served by the existing
   `crates/k_cli/tests/support/static_host.rs`: install verifies the digest and refuses a mismatch,
   installs atomically (an interrupted install leaves nothing), skips yanked versions unless named,
   `list`, `remove` and `default` behave as documented. `K_TOOLCHAIN_INDEX` points at the fake. Then the
   commands. (Signature verification is added when the scheme is chosen; the test for it is written then.)
   *As built (2026-11-12):* `crates/k_cli/src/toolchains.rs` (the commands), `index.rs` (`index.json`
   and choosing a release in it), `archive.rs` (the `.tar.gz` and `.zip` readers, over `flate2`; no
   archive crate), and `k_server::outbound::download`, which streams to a file through the server's
   HTTP client, following redirects, with the SHA-256 computed on the way. Tests:
   `cargo test -p k_cli --test it -- toolchain::commands::` (install, a digest or size mismatch, an
   interrupted download killed halfway, yanked versions, the project's toolchain, `list`, `remove`,
   `default`, `self update`, no index), the unit tests of `index` and `archive`. Decisions in design 48,
   "Commands: decided while implementing": the index format, the `$K_HOME` layout (`bin/k` is a copy
   of the default, recorded in `$K_HOME/default`), a built-in index URL only in release builds (the
   workflows set `K_DEFAULT_TOOLCHAIN_INDEX` to the site's `index.json`; replaced on 2026-11-23 by
   the repository's releases, read through GitHub's API: with the user's token while it was
   private, without one since it is public),
   yanked notes from the last fetched index, and `k upgrade` still never downloading.
5. **Lockfile format versions.** Golden files under `tests/compat/locks/` for today's format 1 of every
   lockfile; tests that a missing `format` reads as 1, a `format` newer than known is the new
   diagnostic, and writing preserves the format read. Then: the `format` key in each writer and reader.
   *As built (2026-11-12):* `k_sema::lockformat` reads and writes the key for every lockfile
   (`FORMATS` lists each file's current format, all 1); the new diagnostic is `K8202 lockfile-too-new`,
   reported at the `format` line instead of anything else about the file, and such a lock is never
   rewritten (`k.lock`'s is reported by the driver, before the command). Format 1 is written without
   the key unless the lock read declared it, so no existing lock changed. Tests:
   `crates/k_server/tests/compat/locks.rs` over `tests/compat/locks/` (a fixture program and each
   file's format-1 golden), the unit and doc tests of `lockformat`, and two `toolchain::` tests in
   `k_cli` for `k.lock`. Decisions in design 48, "Lockfile format versions".
6. **`k fix`.** Tests over the corpus: for every machine-applicable fix in `tests/corpus`, `k fix`
   produces exactly the fixed text the corpus expects and a re-check without that diagnostic; `--code`
   and `--dry-run`; fixes in a workspace library apply to its files; a fix loop terminates. Then the
   command (it reuses `k_diagnostics::Fix` edits).
   *As built (2026-11-12):* the loop is `k_diagnostics::fix` (`select`, `apply`, `fix_until_stable`:
   the first machine-applicable fix of each diagnostic, a fix overlapping one already picked waits for
   the next round, identical fixes count once, at most 8 rounds); `k fix` (`crates/k_cli/src/fix.rs`)
   runs it over the groups `k check` loads, without tests, and writes only the group's own files, so a
   library's fix lands in the library however many members mount it. It then prints what remains as
   `k check` does and exits 1 when anything remains or the rounds ran out; `--dry-run` prints a unified
   diff on standard output. The corpus half: every `sema` case with a machine-applicable fix has a
   blessed `.fixed` expectation (128 cases at first), checked with the same loop, and each fix applied
   alone must lower its code's count on a re-check (`crates/k_corpus/src/sema_suite.rs`). Tests:
   `cargo test -p k_cli --test it -- fix::` (all fixes, `--code`, `--dry-run`, a workspace library),
   `k_diagnostics::fix`'s unit tests (overlaps, a loop of two fixes undoing each other stops).
7. **Stability annotations.** Corpus cases first: `@Deprecated` use is a warning with a fix when the
   replacement's shape matches; `@Preview` without opt-in is an error with a fix adding the feature to
   `k.toml`; a dependency's preview feature not listed by the dependent is an error naming the package;
   a stabilized feature still listed is a warning; `@Internal` is invisible; dependencies' warnings are
   not reported. Then: `k_sema::features` (the feature table), the annotations, and the badges in the
   generated stdlib reference (`book_reference.rs` blessed).
   *As built (2026-11-12):* `@Deprecated(_:since:replacement:)` is `k.core.Deprecated` (any
   declaration or member); `@Preview(_:)` and `@Internal` are in `k.internal`, so only the standard
   library writes them. `k_sema::stability` runs once per revision over the revision's resolutions
   (and the declarations' annotation lists, since annotations aren't resolutions): K1401
   `deprecated` (warning; a fix when the replacement sits beside the deprecated declaration with the
   same labels and the use sees it), K8108 `preview-not-enabled`, K8109 `dependency-preview` (a mounted
   library's `preview` the project lacks), K8110 `stabilized-preview`, and K8101 for an unknown or
   planned id in `preview`. A file without a `k.toml` may use every preview, and the corpus's implicit
   manifest lists them all. `@Internal` keeps a public stdlib declaration out of the prelude and makes
   importing it K1102 outside the stdlib. The first preview row is `workers` (`@Worker`,
   `WorkerFailure`, `WorkerOnlyError`); `examples/notes` opts in. The stdlib reference shows
   `> **Preview** (id).` and `> **Deprecated**` notes and leaves `@Internal` declarations out; the
   design-status page gained a generated Preview table. Corpus cases: `tests/corpus/sema/stability/`.
   Deferred: the stabilized-preview warning and `@Internal` have no real row or declaration yet, so
   they are unit-tested (`stability.rs` with a test table) instead of having corpus cases; "dependencies'
   warnings are not reported" applies to git packages (Planned `packages`), since workspace libraries
   are reported in full by design.
8. **`k upgrade`.** Tests on fixture projects pinned to a fake older minor, with a fake older toolchain
   in the fake index: refuses a dirty tree; applies tombstone fixes; steps through intermediate minors;
   rewrites lock formats; bumps `k` and `k.lock`; prints what remains with the guide's section. Then the
   command.
   *As built (2026-11-12):* `crates/k_cli/src/upgrade.rs`; tests in `crates/k_cli/tests/upgrade.rs`
   (`upgrade::`), the guide lookup's unit tests, and `k_server`'s
   `compat::locks::k_upgrade_moves_every_golden_to_the_current_format`. The project's toolchain is
   `k.lock`'s, else the lowest its requirements allow; the target is the running toolchain or `--to` (a
   version, or a minor meaning its newest installed release). A step on this toolchain computes everything
   in memory and writes it at once: each lockfile through `k_sema::lockformat::upgrade` (the format
   changes' conversions are `lockformat::STEPS`, empty while every lockfile is at format 1), each `k`
   requirement the toolchain doesn't meet raised to its minor (`=` its version for an exact one),
   `k.lock`'s toolchain, then `k fix`'s loop limited to `k_diagnostics::codes::MIGRATIONS` (`K1401`,
   `K8110`; a tombstone's code joins them). Each later minor runs on its newest installed release as
   `k upgrade --to <it> --allow-dirty` with `K_TOOLCHAIN_DELEGATED=1`, and a step that leaves problems
   stops the chain. What remains is printed as `k check --no-tests` would, each code that a generated
   section of the (embedded) upgrade guide names with that section; exit 1 when anything remains.
   Decisions in design 48, "`k upgrade`: decided while implementing". The fake toolchains are installed
   directories rather than an index: `k upgrade` never downloads (decided with step 4), so it names
   `k toolchain install <minor>` for a missing one and stops before changing anything. Tombstones wait
   for the first removal.
9. **Changelog fragments and `xtask`.** Tests in `xtask`: fragment parsing and validation; compiling
   fragments into a `CHANGELOG.md` section, the changelog page and an upgrade-guide section (golden
   output); `prepare` refusing minor-only kinds in a patch; `prepare` updating every version string so
   step 1's test passes. Then `changes/` with a README, the `xtask` crate, and fragments for the changes
   since the last release.
   *As built (2026-11-12):* `crates/xtask` (modules `fragment`, `changelog`, `bump`, `release`, `gate`),
   run as `cargo xtask` through `.cargo/config.toml`; tests: `cargo test -p xtask` (unit tests, and
   `fragments::`, `prepare::` with goldens under `crates/xtask/tests/golden/release/`, `bump::`, `gate::`).
   `release prepare <version>` checks every fragment, refuses `deprecation`, `language`, `stdlib` and
   `preview` in a patch (warning on `breaking`), bumps every string `version.rs` checks (`Cargo.toml`,
   the workspace's packages in `Cargo.lock`, the extension's `package.json` and `package-lock.json`, the
   JetBrains plugin's `gradle.properties`, the
   examples' `k`, `k.lock` and image tags, the book's `toml` samples, `k X.Y.Z` mentions and image tags,
   design 38), writes the release's `CHANGELOG.md` section (replacing its candidate's), the changelog
   page's copy and the upgrade guide's entries for the release, and for a release (not a candidate)
   deletes the fragments and empties `_highlights.md`; then it runs `version::` unless `--no-verify`.
   `release check` runs the local gate (`--list` prints it, `--release` also requires that no fragment
   remains); `release start-next` only bumps; `docs-gate <base>` and `fragments` are as in §3. The
   fragments for 0.1.0 are one per area of what is built, seeded from the history rather than one per
   commit; their `book:` pages describe the area and aren't in this change's diff, so the docs gate
   applies from the next change on. Decisions in design 48, "Changelog fragments: decided while
   implementing".
10. **Wire compatibility fixtures.** Record `tests/compat/wire/0.1/` from the examples (rpcs with
    declared errors and `rpcFailure`, a server stream, a service-token call, a queued job row, a page's
    `k-data`); tests that the current server answers each recorded request as recorded and the current
    client decodes each recorded response. These pass today and start guarding the next change.
    *As built (2026-11-12):* `crates/k_server/tests/compat/wire.rs` records and replays
    `tests/compat/wire/0.1/paybox.json` (`examples/shop/paybox`: service-token calls, a declared error
    with a payload, `rpcFailure` without a token and for an invalid message, the queued `k_job` row,
    which the current worker then runs, and the delivery it makes) and `notes.json` (`examples/notes`:
    the session cookie, a guest's `rpcFailure`, a server stream's envelopes, a declared error, the
    page's `k-data`), with the current browser client decoding each recorded response and starting
    from the recorded `k-data`. Volatile values are compared by shape and recorded ids mapped to the
    run's; a response may add fields. Decisions in design 48, "Wire and stored formats across a
    rolling deploy".
11. **Install scripts.** Tests first (run by `cargo test` where `sh` exists, and in CI on each OS):
    `install.sh` against the fake release picks the right archive for `uname` (and fails clearly on an
    unsupported target such as musl), verifies `SHA256SUMS`, installs into `~/.k/bin`, prints the
    `PATH` line, and is idempotent; the same for `install.ps1` on Windows. Then the scripts.
    *As built (2026-11-12):* `scripts/install.sh` (POSIX `sh`; `curl` or `wget`, `sha256sum` or
    `shasum`, `tar`) and `scripts/install.ps1` read `K_RELEASES`, a host laid out like GitHub Releases
    (`latest/download/<file>` for the newest stable, `download/v<version>/<file>`), take `--version`
    (`-Version`) or `K_VERSION`, install into `$K_HOME/toolchains/<version>/` with the same layout as
    `k toolchain`, make it the default and print the `PATH` line without editing any profile.
    *Since 2026-11-23:* with no version asked for and no stable release (no `latest/download/`), they
    read `K_TOOLCHAIN_INDEX` (the site's `index.json`, filled in by the docs build) and install the
    newest release there that isn't yanked and has this machine's archive, a candidate before a
    nightly, saying so; the download and the `SHA256SUMS` check are unchanged. Tests:
    `cargo test -p k_cli --test it -- install_script::`, with fake `uname` and `ldd` first on the `PATH`
    so every Unix target is tried on one machine; the `install.ps1` test runs where `pwsh` is installed
    (CI's Windows job) and says it was skipped elsewhere. `scripts/affected-tests.sh` maps a change in
    `scripts/` to the tests that name the directory.
    *Since the private distribution (2026-11-23):* `K_RELEASES` and `K_TOOLCHAIN_INDEX` are gone. The
    scripts reach `slavkobojanic/kobra` with `gh` (`gh api` for JSON, `gh release download` for
    assets), or without `gh` with curl and `GH_TOKEN`/`GITHUB_TOKEN` through the REST API
    (`K_GITHUB_API` points it at a fake; `K_INSTALL_GH=none` forces curl); they check access first (a 404
    says "you may not have access to slavkobojanic/kobra"), take the newest stable from
    `releases/latest`, else the newest candidate from the newest release's `index.json`, and unpack
    `docs-<version>.tar.gz` into `$K_HOME/docs/<version>/`. The tests run them against
    `support/release.rs`'s fake release laid out as the API serves it, with a fake `gh`
    (`release::fake_gh`: signed in, signed out, no access) first on the `PATH`, and for curl a
    loopback API whose asset URLs redirect to a second host, which must never see the token.
    *Since the public distribution (2026-11-23):* `K_RELEASES` and `K_TOOLCHAIN_INDEX` are back, as
    test overrides of the defaults the scripts name (`https://github.com/slavkobojanic/kobra/releases`,
    `https://slavkobojanic.github.io/kobra/index.json`). The scripts download with plain curl or wget
    (`Invoke-WebRequest`), with no token: `latest/download/SHA256SUMS`, then that release's
    `download/v<version>/…`; with no stable release, the site's `index.json`, else the newest
    release's found through the API (`K_GITHUB_API`), the one request that carries a token when one is
    found (`GH_TOKEN`, `GITHUB_TOKEN`, `gh auth token`; `K_INSTALL_GH=none` skips `gh`). The access
    check is gone. Tests: a fake release, docs site, API and storage host on loopback; every install
    needs neither `gh` nor a token, and no host but the API ever sees one.
12. **Docs version substitution and the docs build.** Tests: `{{k_version}}` is replaced in every page;
    `versions.json` is generated from the tags and branch; the site layout (`/0.N/`, `/next/`, redirects
    for `/` and `/latest/`) is produced by `cargo xtask docs build --version 0.1 --out …` into a
    directory, checked by a test. Coordinate with [design 49](../design/49-documentation.md)'s version
    switcher and redirects.
    *As built (2026-11-12):* `crates/xtask/src/docs.rs`; tests: `cargo test -p xtask --test it --
    docs::` (a two-page fixture book in `crates/xtask/tests/fixtures/book/` built as a minor and as
    `next` into one directory, and the real book, every page checked for a leftover `{{k_version}}` and
    for the version switcher). One run builds one version: `--version` is `next`, `latest` (the newest
    stable minor of the `v*` tags, built from its newest tag), a minor, a version or a tag (`v0.6.1`
    names `0.6`); `--from <ref>` builds another commit's book through a temporary `git worktree`. A
    minor's book must come from a commit of that minor (else an error naming `--from`). The book is
    copied to a scratch directory with `{{k_version}}` replaced by the source's minor before mdBook runs,
    so a plain `mdbook build docs/book` needs nothing from `xtask`; any `ERROR` or `WARN` line of mdBook
    fails the build (mdBook 0.5 exits 0 after some errors, such as an unknown `book.toml` key). Each run
    then rewrites the root: `versions.json` (`format`, `latest`, `live`, `next`; the live minors are
    the stable tags' and the directories already built, before 1.0 only the newest, from 1.0 every
    minor from 1.0 on), `index.html` and one redirect page per page of the newest minor under
    `latest/` (relative, so the site works below any path; `next/` before any release), a `404.html` that
    sends `latest/…` and a retired minor's pages to the newest minor's same page and a missing page of a
    live version to its landing page, `.nojekyll`, and the install scripts, taken from the newest minor's source, or the first build's
    (until 2026-11-23 with `@K_RELEASES@` and `@K_TOOLCHAIN_INDEX@` filled in from `--releases` and
    `--index`; the scripts now need nothing filled in, and the options are gone). The theme's
    `theme/k-version.js` reads `versions.json` to add the version switcher (same page in another version)
    and the banner on `/next/` and on an archived minor; it does nothing in a book outside a version
    directory. Decisions in design 48, "Docs versioning: decided while implementing". The lexer
    highlighting preprocessor of [documentation.md §4](documentation.md#4-highlighting-planned) and
    Pagefind (an open owner decision) are not part of it.
13. **CI workflows.** `.github/workflows/ci.yml`, `nightly.yml`, `release.yml`, `publish.yml` as in §9,
    each step a `cargo` or `cargo xtask` command. A test parses them and checks that every gate item of
    design 48 appears in the release workflow, so the gate and the pipeline can't drift. Run them for the
    first time once the owner has chosen the CI host.
    *As built (2026-11-12):* `.github/workflows/{ci,nightly,release}.yml`, the local actions
    `.github/actions/{setup,site}/action.yml`, `scripts/ci/*.sh`, `docker/toolchain.Dockerfile`, and in
    `xtask` the commands they run: `dist archive` (a reproducible `.tar.gz` or `.zip`, written by
    `xtask/src/dist.rs` over `flate2`), `dist checksums`, `dist index` and `dist merge-index` (the
    `index.json` format of `crates/k_cli/src/index.rs`), `dist smoke`, `release nightly`, `fragments
    --none-left`, `ci changed`, `ci version` and `ci notes`. Tests: `cargo test -p xtask --test it --
    workflows:: dist::`. `workflows.rs` reads the YAML with `yaml-rust2` (`xtask/src/workflows.rs`) and
    checks everything listed in [§9, "Running CI"](#running-ci); its gate check reads design
    48's gate table and requires each row's command in the workflow of each column that runs it
    (`ci.yml` for pull requests, `nightly.yml`, `release.yml`), through a table of the text each row
    runs, so a new gate row fails the test until it is wired in. `dist::` unpacks the archives with the
    system's `tar` and `unzip`. Decisions in design 48, "The pipeline: decided while implementing":
    every job off behind `K_CI_ENABLED`, with `K_PAGES_ENABLED`, `K_PUBLISH_ENABLED` and `K_PERF_RUNNER`
    for the rest; `publish.yml` folded into `release.yml`; the index assembled from each release's
    `index-entry.json`; the toolchain image on `debian:bookworm-slim`. **Never run:** nothing here has
    run on GitHub; the commands were run locally where they can be (`cargo xtask docs build`, `dist
    smoke` against a debug `k`, the archive and index steps in the tests), and the first enabled run
    will need fixes. Not built: the nightly's image tag and extension pre-release, skipping an
    unchanged nightly, marketplace and package-manager jobs, yanking (`release yank`).
    *Revised (2026-11-23), CI on request:* every workflow runs only on `workflow_dispatch`, and
    `K_CI_ENABLED` is gone; `release.yml` takes the version as an input, checks it first
    (`cargo xtask ci check-version`, unit-tested in `xtask/src/release.rs`), and creates and pushes the
    tag itself in its `release` job (`scripts/ci/tag.sh`) after the gate and the builds; the
    `release: published` jobs moved to `publish.yml`; `K_PUBLISH_ENABLED` now guards only the image
    pushes. Tests: `workflows::` (the trigger, the inputs, the one tagging job and its `needs`, the tag
    script against a local `origin`, the publish workflow). Design 48, "CI on request".
14. **Extension version and `serverInfo`.** Test in `k_lsp/tests/server.rs` that `initialize` returns
    `serverInfo.version`; the extension shows it. The extension's version is checked by step 1.
    *As built (2026-11-12):* the server already answered `serverInfo: { name: "k", version }` with the
    workspace version; `server::initialize_reports_the_toolchain_version` now pins it against
    `Cargo.toml`. The extension (`editors/vscode/extension.js`) shows it in a language status item for K
    files (`K 0.1.0`, "toolchain (k lsp)"; "K: not running" when the server can't start; clicking it
    restarts the server), and `editors::the_vscode_extension_shows_the_servers_version` checks that it
    reads `initializeResult.serverInfo`. The item is not exercised in a running VS Code by any test.
15. **First release dry run.** With every step above, run §6 end to end on a local branch as
    `0.1.0-rc.1`: the release workflow's jobs locally through `cargo xtask release check` and the
    per-target smoke test on the machines available, without publishing. Record what needed doing by
    hand and fix this runbook.
    *As built: 0.1.0-rc.1 dry run (2026-11-12).* Run in two scratch clones of main (`bbbbfd4b`, then
    with the fixes below), never this working copy, with the clones' remote removed: nothing was
    pushed or published, no account was used, and the tag `v0.1.0-rc.1` existed only in the clones,
    which were deleted afterwards. One machine was available, macOS on `aarch64-apple-darwin`.

    | §6 step | What ran | Result |
    |---|---|---|
    | 2. cut | `git switch -c release/0.1` | ok |
    | 3. prepare | `cargo xtask release prepare 0.1.0-rc.1` | 34 files: the versions, the examples' locks and image tags, `CHANGELOG.md`'s `## 0.1.0-rc.1 (pre-release of 0.1.0, 2026-11-12)`, the changelog page, the upgrade guide's "Upgrading to 0.1" ("nothing to migrate"); `version::` passed. It missed the test projects (below) |
    | 4. notes | read the draft | the 18 fragments compile into 6 sections (internal ones are dropped); no highlights had been written, so they are now in `changes/_highlights.md` for 0.1 |
    | 5. gate | each line of `release check --list` | formatting, `k fmt --check examples` (616 files), the real-Chrome suites (90 passed) and the docs build passed; `cargo test --workspace` failed 31 tests, all fixed below; with the fixes, 6,791 passed, 0 failed, 51 ignored at `0.1.0-rc.1` |
    | 6. commit, tag | `git commit -am`, `git tag -a v0.1.0-rc.1` in the clone | ok; not pushed |
    | 7. bump main | `cargo xtask release start-next 0.2.0` on a fixed clone, then the suite | 123 files; 44 `k_cli` tests failed (below), and with that fixed all 317 passed at `0.2.0` |
    | 8. pipeline, by hand | `cargo build --release -p k_cli --locked` (1 min 20 s), `dist archive`, `dist checksums`, `dist index`, `dist merge-index`, `dist smoke`, `ci version`, `ci notes`, `docs build --version v0.1.0-rc.1`, and `gate-fuzz` for `lex` and `lsp_edits` | a 28.5 MB `k-0.1.0-rc.1-aarch64-apple-darwin.tar.gz`, byte-identical when packed twice; the smoke test passed on the unpacked `k`; the site built `/0.1/` (116 chapters) with the version substituted, `versions.json`, the redirects and the install scripts with the host filled in; `docs build --version latest` with only a candidate tag writes only the root files, as it should |
    | 9. install, as a user would | `python3 -m http.server` on `127.0.0.1:5311` serving `releases/download/v0.1.0-rc.1/` and `index.json`; `install.sh` (`--version`, `K_VERSION`, again), `k toolchain install` (named, by minor, from the project's `k.lock`), `list`, `default`, `remove`, `k self update` with `K_TOOLCHAIN_INDEX` | the candidate installs by name into `$K_HOME/toolchains/0.1.0-rc.1/` and becomes the default; installing it again changes nothing; the messages without a name were wrong (below) |

    **Found and fixed**, each with its test first: `k new` on a candidate wrote `k = "0.1.0-rc"`
    (`new::tests`, `book_projects::`), and `k check` accepted it silently, as it accepted any malformed
    app `k.toml`: an app's manifest problems are now reported (`K8101`; `new::k_check_reports_a_malformed_app_manifest`);
    `k upgrade` running on a candidate refused to step through the candidate's own minor
    (`upgrade::tests::the_running_candidate_counts_for_its_own_minor`); `release prepare` and
    `start-next` didn't move the test projects' `k` and `k.lock`, so `k build` refused them with K8201
    (`bump::`, `version::test_projects_require_the_current_minor`), and the manifests `k_cli`'s tests
    write said `k = "0.1"` literally (now `k_requirement!()`, checked by
    `version::test_sources_write_no_literal_requirement`); `crates/k_server/tests/projects/channels/.env`
    was git-ignored, so a fresh checkout failed 25 channel tests (`checkout::`); `fuzz/corpus` is
    git-ignored, so `gate-fuzz` had no corpus in a fresh checkout, and `lsp_edits` had no seeds at all
    (the jobs run `fuzz/seed-corpus.sh` first, which seeds every target and fails on an empty one;
    `workflows::`); the release notes linked `docs/book/src/…` relatively, which a release page doesn't
    resolve (`ci notes --links`; `release::tests`); with only a candidate published, the install
    script failed on a bare 404 and `k toolchain install` said "has no a stable release" (both now say
    there is no stable release and name the candidate or `--version`; `install_script::`,
    `index::tests`); the compiler-only `k` (`--no-default-features`) built with four dead-code
    warnings. Two expectations were wrong for a pre-release and now say so
    (`toolchain::a_patch_difference_runs_and_k_build_updates_the_lock`, the minor in `new::`).

    **Done by hand**, which the pipeline does on GitHub: serving the release host and the index
    (`release-index.sh` needs `gh`), merging the one index entry, and taking the install scripts from
    the built site. **Not run here:** the other five targets' builds and smoke tests (no such
    machines), `install.ps1` (no `pwsh`), the `.vsix` (`vsce` isn't installed and npm would need the
    network; whether the Marketplace takes a pre-release version is open until the marketplaces are
    chosen), the toolchain image (it needs a Linux archive), signing and `verify.sh` (Sigstore's
    keyless signing needs the workflow's identity), `gate-perf`, `gate-properties`, and the fuzz gate
    beyond `lex` and `lsp_edits` (the local fuzz build needs `RUSTY_V8_ARCHIVE`, [testing.md §6.2](testing.md#62-targets)). The
    Firefox recording test (`firefox::media_records_each_named_format_in_firefox`, the recording decodes
    1.2 to 1.4 s) failed in two of three full runs and passes alone: a flake under load, not yet
    explained ([testing.md §4.6](testing.md#46-flakes)); the gate has no retries, so it can fail a release check.

    **Settled since (2026-11-23):** the name (Kobra; the command stays `k`, files `.k`), the namespace
    (`github.com/slavkobojanic/kobra`, `ghcr.io/slavkobojanic/kobra`, the extension
    `slavkobojanic.kobra`) and the licence (MIT OR Apache-2.0, both files in every archive); design 48,
    "Owner decisions". Then, the same day, **private distribution** (design 48, "Private
    distribution"): the repository stays private; collaborators install with the GitHub CLI
    (`gh api …/contents/scripts/install.sh … | sh`); `k toolchain` and `k self update` read the
    releases through GitHub's API with `GH_TOKEN`, `GITHUB_TOKEN` or `gh auth token`, sent to the API
    only (`crates/k_cli/src/github.rs`; tests `toolchain::commands::`); every release attaches its book
    (`cargo xtask dist docs`) and the whole `index.json`; `k docs` opens the installed book
    (`docs_command::`); no workflow runs on macOS, and `cargo xtask release attach-local` builds and
    attaches the macOS archive from the owner's Mac (§6; tests `dist::attach_local_*` with a fake
    `gh`); the docs site, `K_DEFAULT_TOOLCHAIN_INDEX` and the docs build's `--releases`/`--index` are
    gone, the Pages jobs kept behind `K_PAGES_ENABLED` for the day the repository is public.
    Then, later the same day, **public distribution** (design 48, "Public distribution"): the
    repository is public, Pages is enabled and `K_PAGES_ENABLED=true`; the one-liners are
    `curl -fsSL https://slavkobojanic.github.io/kobra/install.sh | sh` and
    `irm https://slavkobojanic.github.io/kobra/install.ps1 | iex`, served at the site's root by the
    Pages deploys of `ci.yml` (a run of main) and `publish.yml`; the scripts and `k toolchain` download
    without a token (one, if found, still goes to the API only; tests `install_script::`,
    `toolchain::commands::`); `k docs` stays and, without the book, names the site
    (`docs_command::`). CI stays `workflow_dispatch` only, with no macOS runners.

    **What only the owner can do** before a real `0.1.0-rc.1`
    ([design 48, "Owner decisions"](../design/48-releases-and-versioning.md#owner-decisions)): rename
    the GitHub repository to `slavkobojanic/kobra` (GitHub redirects the old name) and push `main`
    (done; it is public, with Pages and `K_PAGES_ENABLED` on); decide on image pushes
    (`K_PUBLISH_ENABLED`); choose the required checks and the performance runner
    (`K_PERF_RUNNER`); check that the Sigstore identity `verify.sh` expects is the repository's;
    prepare `0.1.0-rc.1` and push it (§6 steps 3–6), then `gh workflow run release.yml -f
    version=0.1.0-rc.1` (add `--ref release/0.1` if the branch is cut), the first real pipeline run,
    expected to need fixes since no workflow has run on GitHub yet; then attach the macOS archive
    (`cargo xtask release attach-local v0.1.0-rc.1`, §6 step 9) and publish
    (`gh workflow run publish.yml -f version=0.1.0-rc.1`). Until a release is published and Pages deployed, `curl -fsSL https://slavkobojanic.github.io/kobra/install.sh | sh` has nothing to install (start `ci.yml` on main, or `publish.yml`, to deploy the site); the script says so once it runs.
