# Releasing K

The maintainer runbook: how a version of K is cut, checked, published and fixed. The decisions behind it
(what a version promises, channels, pinning, upgrades, formats, the docs) are in
[docs/design/48-releases-and-versioning.md](../design/48-releases-and-versioning.md); this file is the
procedure.

> **Status.** There are no tags, no CI configuration and no published artifacts, and the workspace
> version is `0.1.0`. Steps 1–3 and 5–10 of the [implementation plan](#12-implementation-plan) are
> built: version consistency, the toolchain requirement, `k.lock`'s toolchain with delegation, lockfile
> format versions, `k fix`, the stability annotations with the feature table, `k upgrade`, the changelog
> fragments with `cargo xtask` and the 0.1 wire fixtures. Sections marked *(planned)* describe files and
> commands that the [implementation plan](#12-implementation-plan) adds. Publishing anything needs the
> owner decisions of [design 48](../design/48-releases-and-versioning.md#owner-decisions); every step up
> to publishing can be run locally.

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
| `changes/*.md` | changelog fragments, one per change; `changes/README.md` explains them | exists (the 0.1.0 fragments, one per area) |
| `changes/_highlights.md` | hand-written release highlights, emptied at each release | exists |
| `CHANGELOG.md` | every released version's notes, newest first | exists (no version released yet) |
| `crates/xtask/` | `cargo xtask release prepare`, `check`, `start-next`; `cargo xtask docs-gate`, `fragments` | exists (`release sync-main` and `release yank` planned) |
| `.github/workflows/ci.yml`, `nightly.yml`, `release.yml` | the pipeline (§9); GitHub Actions is the default, an owner decision | planned |
| `scripts/install.sh`, `scripts/install.ps1` | installers, served from the docs site | planned |
| `tests/compat/locks/`, `tests/compat/wire/` | golden lockfiles per format and recorded traffic per minor; checked by `crates/k_server/tests/compat/` | exists (format 1 of every lockfile; the 0.1 transcripts) |
| `crates/k_cli/tests/version.rs` | every version string agrees; `k --version` format | exists (the commit and date in `--version` come with step 1's `build.rs`, in round 3) |
| `k.lock` (`toolchain = "…"`) in every example app and workspace root | the toolchain that last built it; `version.rs` checks it names the version | exists |
| `crates/k_cli/tests/toolchain.rs` | requirement, delegation, `K_TOOLCHAIN`, `k.lock` updates against fake installed toolchains; `k toolchain` against a local fake index with step 4 | exists (step 4's tests to come) |
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
- **Tags** are `vX.Y.Z` and `vX.Y.Z-rc.N`, annotated, on the release branch. **A tag is never moved or
  reused**; a bad release is yanked and fixed by the next patch (§11).
- **Only the release pipeline publishes.** Nothing is uploaded from a maintainer's machine.

---

## 6. Cutting a minor release

Timeline for `0.6.0`, with T the planned release day.

### T−7: branch cut and release candidate

1. **Check main is green:** the nightly of the commit you will cut passed (fuzz, long property runs,
   performance budgets included). If it didn't, the cut waits; the train doesn't ship red.
2. **Cut the branch:**
   ```sh
   git switch main && git pull
   git switch -c release/0.6
   ```
3. **Prepare**:
   ```sh
   cargo xtask release prepare 0.6.0-rc.1
   ```
   It sets the version everywhere ([§2](#2-files-the-release-process-owns)), compiles the fragments
   into a draft `CHANGELOG.md` section, the changelog page and the upgrade guide's new section (it does
   not delete fragments for a release candidate), updates `k = "0.6"` in the examples and the `k new`
   skeleton, runs `cargo test -p k_cli --test it -- version::` and prints what it changed. It will also
   print the book pages whose audit-ledger entry predates the previous minor's cut and whose design topic
   changed since *(planned: that needs a previous release tag, so it comes with the first minor after
   0.1)*: re-audit those before tagging the candidate.
4. **Review the draft notes:** read every fragment in the context of the others, fix wording in the
   fragments themselves, and write `changes/_highlights.md` (three to six bullets; anything operators
   must know first, such as a new wire encoding being written).
5. **Run the local gate:**
   ```sh
   cargo xtask release check
   ```
   which is `cargo fmt --all --check`, `cargo clippy --workspace --all-targets`, `cargo test --workspace`,
   `K_REQUIRE_CHROME=1` real-browser suites, `mdbook build docs/book` with no warnings, the compat suite
   and the fragment checks. It is the same list CI runs, so a failure here saves a pipeline run.
6. **Commit and tag:**
   ```sh
   git commit -am "Release 0.6.0-rc.1"
   git tag -a v0.6.0-rc.1 -m "K 0.6.0-rc.1"
   git push origin release/0.6 v0.6.0-rc.1
   ```
7. **Bump main** to the next minor in its own pull request: `cargo xtask release start-next 0.7.0`
   sets `0.7.0` and leaves the fragments on main untouched.
8. The release pipeline (§9) builds the candidate and publishes it as a **pre-release**: archives,
   checksums, signatures, the index entry (marked `rc`), the extension `.vsix`, the image tag
   `0.6.0-rc.1`. No docs are deployed for a candidate; `/next/` already shows main.
9. Announce the candidate with `K_TOOLCHAIN=0.6.0-rc.1` instructions, and ask that `k upgrade --to
   0.6.0-rc.1` be tried on real projects.

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
2. `cargo xtask release check`, then commit, tag `v0.6.0`, push the branch and the tag.
3. The release pipeline runs the full gate on the tag on every target and builds every artifact. It
   creates a **draft** release; nothing is visible yet.
4. **The release manager reviews the draft**: every target's smoke test passed, `SHA256SUMS` lists every
   archive, the signature verification job passed, the notes render, the docs build preview for `/0.6/`
   looks right (version switcher, upgrade guide section, changelog page).
5. **Publish the draft.** This one manual step triggers the publish jobs: the index entry, the image tags
   `0.6.0`, `0.6`, `latest`, the extension to its marketplaces (once chosen), package-manager updates
   (once chosen), and the docs deploy of `/0.6/` with `/` and `/latest/` pointing at it.
6. **Merge the release back:** a pull request from `release/0.6` to main carrying `CHANGELOG.md` and the
   book's new changelog and upgrade-guide sections, and deleting on main the fragments that shipped
   (`cargo xtask release sync-main` *(planned)* computes the list).

---

## 7. Cutting a patch release

1. The fix is on main, with its fragment (`kind: fix`, or `breaking` for a soundness or security fix).
2. Cherry-pick it to `release/0.6`, fragment included.
3. `cargo xtask release prepare 0.6.1`, `cargo xtask release check`, commit, tag `v0.6.1`, push.
4. Steps 3–6 of "T: the release" above. The docs deploy replaces `/0.6/`.

`cargo xtask release prepare` refuses a patch whose fragments include `deprecation`, `language`,
`stdlib` or `preview` kinds, and warns on `breaking`, so a minor's changes can't slip into a patch.

A **docs-only fix** to a released minor needs no tag: merge it into `release/0.6` and run the pipeline's
"deploy docs" job for that branch, which rebuilds `/0.6/` after the branch's own tests passed.

---

## 8. Nightly

- **Scheduled once a day** on main, skipped if main hasn't changed since the last nightly.
- Runs the nightly column of the gate: every target's tests, fuzzing for a fixed time per target, long
  property runs, performance budgets on the dedicated runner, the smoke tests and the image build.
- **On green**, publishes `0.7.0-nightly.<date>` (archives, index entry under `nightly`, the image tag
  `nightly`, the extension as a pre-release) and rebuilds `/next/`.
- **On red**, publishes nothing, opens or updates one issue with the failing jobs, and blocks the next
  branch cut until a later nightly is green.
- New fuzz crashes are minimised and committed under `fuzz/artifacts/` with a regression case in the
  corpus ([testing.md §6–7](testing.md#6-fuzzing)) before the issue is closed.

---

## 9. The pipeline

The default design is GitHub Actions (owner decision; the jobs translate directly to another CI system).
Every job runs `cargo` commands that also work locally; YAML holds no logic that `cargo xtask` doesn't.

### `ci.yml`: every pull request and every push to main or a release branch

| Job | Runner | Runs |
|---|---|---|
| `lint` | Linux x86_64 | `cargo fmt --all --check`, `cargo clippy --workspace --all-targets` |
| `test-linux` | Linux x86_64, with a Postgres service and Chrome | `cargo test --workspace` with `K_TEST_POSTGRES_URL` and `K_REQUIRE_CHROME=1` |
| `test-macos` | macOS arm64 | `cargo test --workspace` |
| `book` | Linux x86_64 | `mdbook build docs/book` (warnings fail), the freshness checks (they are inside `cargo test`; this job also runs the link check) |
| `fragments` | Linux x86_64 | a fragment exists when `crates/`, `editors/` or `docs/book/` changed; `cargo test -p xtask`; `cargo xtask docs-gate <base>` |
| `site` | Linux x86_64; only when `docs/book/` or `crates/xtask/` changed | `cargo xtask docs build --version next`; the built site uploaded as an artifact (the PR preview until the owner picks a host with previews) |
| `install-scripts` | Linux, macOS, Windows; only when `scripts/install.*` changed | each installer against a fake release served from a local directory |

Caching: the cargo registry and `target/` keyed by `Cargo.lock` and the runner; the V8 prebuilt library
that `deno_core` downloads, keyed by its version.

### `nightly.yml`: scheduled

| Job | Runner | Runs |
|---|---|---|
| `test-<target>` | one per target ([design 48](../design/48-releases-and-versioning.md#artifacts)) | `cargo test --workspace` |
| `properties` | Linux x86_64 | the property suites with raised `PROPTEST_CASES` ([testing.md §2](testing.md#2-running-tests)) |
| `fuzz` | Linux x86_64 | `cargo +nightly fuzz run <target> -- -max_total_time=600` for each target |
| `perf` | the dedicated performance runner | `scripts/bench-check.sh` |
| `build-<target>` | one per target | `cargo build --release -p k_cli --locked`, archive, then the smoke test |
| `image` | Linux x86_64 and arm64 | build the toolchain image, then build `examples/shop` with it |
| `publish-nightly` | needs every job above | archives, index, image tag, extension pre-release, `/next/` |

### `release.yml`: a pushed `v*` tag

1. **`gate`**: the nightly jobs' tests, properties (shorter run), fuzz **corpus replay** (deterministic:
   each target run over its committed corpus, no new inputs), perf, the book, the compat suite, the
   version and fragment checks with "no fragment may remain".
2. **`build-<target>`**: release build, `--locked`, on the target's native runner; Linux against the
   glibc 2.31 baseline; archive named `k-<version>-<target>.tar.gz` (`.zip` on Windows).
3. **`smoke-<target>`**: on a fresh runner of the target: unpack, `k new hello`, `k check`, `k test`,
   `k build`, start `dist/app`, `dist/app probe http://127.0.0.1:<port>/…`.
4. **`checksums`**: `SHA256SUMS` over every archive.
5. **`sign`**: signatures and build provenance for every archive (the scheme is an owner decision;
   keyless Sigstore needs only the workflow's OIDC token, `id-token: write`), then a **`verify`** job that
   downloads the artifacts as a user would and verifies digests and signatures.
6. **`extension`**: package `editors/vscode` into `k-language-<version>.vsix` (npm is used here and only
   here, [architecture.md §15](architecture.md#15-dependencies)).
7. **`image`**: multi-arch toolchain image, pushed under the version tag only (not `latest` yet).
8. **`docs`**: build the book from the tag with the version substituted; upload it as an artifact for
   review (not deployed yet).
9. **`draft`**: a draft release with the notes from `CHANGELOG.md`, every archive, `SHA256SUMS`,
   signatures and the `.vsix`.

**On publishing the draft** (`release: published`), `publish.yml` runs: the index entry, the image's
`X.Y` and `latest` tags (not for a candidate), the docs deploy and `versions.json`, and the marketplace
and package-manager jobs that the owner has enabled. Each publish job is idempotent, so a failed one is
re-run on its own.

**Permissions:** the gate and build jobs have read-only tokens; only `sign` (`id-token: write`),
`draft` (`contents: write`), `image` (`packages: write`) and the publish jobs get write access. Marketplace
and package-manager tokens are environment secrets, scoped to a `release` environment that requires a
maintainer's approval.

---

## 10. After publishing

1. **Install as a user would**, on a machine without K: the install script from the docs site, then
   `k --version`, `k new hello && cd hello && k dev`.
2. **Upgrade as a user would:** on a project pinned to the previous minor, `k toolchain install`,
   `k upgrade`, `k check`. Note anything `k upgrade` couldn't fix that the upgrade guide doesn't cover,
   and fix the guide on the release branch.
3. **Check the docs:** `/` lands on the new minor, the version switcher lists it, `/next/` still builds
   from main, an old minor's URL redirects (before 1.0).
4. **Close the release checklist issue** with the performance numbers and links to the artifacts.

---

## 11. When a release is bad

- **Never delete or re-tag.** Projects' `k.lock` files and other people's caches name the version.
- **Yank:** mark the version `yanked` in the index (`cargo xtask release yank 0.6.1` *(planned)* opens
  the pull request; the publish job deploys the index). `k toolchain install` then skips it unless it is
  named exactly, and `k toolchain list` and the delegation diagnostic say it is yanked, with the reason.
  Its archives stay downloadable, so pinned builds still reproduce.
- **Fix forward:** the next patch, through §7, with a fragment explaining the problem and the fix.
- **A wrong docs deploy:** rerun the docs job from the release branch; the docs are a pure function of
  the branch.
- **A security problem:** reported privately (the address is in `SECURITY.md` *(planned)*); fixed in a
  private fork; patch releases are prepared for the newest minor and, when it is affected, the previous
  one; published together, with an advisory. The fragment is written only in the private fork.

---

## 12. Implementation plan

Each step starts with failing tests, in the style of [testing.md](testing.md). No step publishes anything
or needs an account; step 13 writes CI files that run once a host is chosen.

**Scheduling:** these steps are run in the rounds of the combined plan in
[design 49](../design/49-documentation.md#implementation-plan), together with the documentation work:
step 1 in round 1; steps 2, 3, 5, 6, 7, 9, 10 in round 3; steps 4, 8, 11, 12, 13, 14 in round 4; step 15
in round 5.

1. **Version consistency.** Test first: `crates/k_cli/tests/version.rs` asserts that `k --version`
   prints `k <version> (<commit> <date>)` and that `Cargo.toml`, `editors/vscode/package.json`, the
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
   directories rather than an index: installing a missing toolchain is step 4's, so until then `k upgrade`
   names `k toolchain install <minor>` and stops before changing anything. Tombstones wait for the first
   removal.
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
12. **Docs version substitution and the docs build.** Tests: `{{k_version}}` is replaced in every page;
    `versions.json` is generated from the tags and branch; the site layout (`/0.N/`, `/next/`, redirects
    for `/` and `/latest/`) is produced by `cargo xtask docs build --version 0.1 --out …` into a
    directory, checked by a test. Coordinate with [design 49](../design/49-documentation.md)'s version
    switcher and redirects.
13. **CI workflows.** `.github/workflows/ci.yml`, `nightly.yml`, `release.yml`, `publish.yml` as in §9,
    each step a `cargo` or `cargo xtask` command. A test parses them and checks that every gate item of
    design 48 appears in the release workflow, so the gate and the pipeline can't drift. Run them for the
    first time once the owner has chosen the CI host.
14. **Extension version and `serverInfo`.** Test in `k_lsp/tests/server.rs` that `initialize` returns
    `serverInfo.version`; the extension shows it. The extension's version is checked by step 1.
15. **First release dry run.** With every step above, run §6 end to end on a local branch as
    `0.1.0-rc.1`: the release workflow's jobs locally through `cargo xtask release check` and the
    per-target smoke test on the machines available, without publishing. Record what needed doing by
    hand and fix this runbook.
