# Releases, Versioning and Upgrades

## Decision

**Kobra has one version number for everything it ships:** the `k` binary, the standard library and the JavaScript runtime embedded in it, the VS Code extension, the toolchain container image and the book. A release is one commit, tagged `vX.Y.Z`, from which every artifact and the documentation are built, after one gate has passed on that commit.

- **Semantic versioning, Cargo-style before 1.0:** `0.Y` is the compatibility unit, so `0.5.0 → 0.6.0` may break and `0.5.0 → 0.5.3` never does. From 1.0, majors break, and editions make a major rare.
- **Two channels, stable and nightly**, with a release candidate before each stable minor. Minors ship on a six-week train once the first release is public.
- **A project pins its toolchain:** `[package] k = "0.5"` is a requirement, and `k.lock` records the exact toolchain version that last built the project. An installed `k` that doesn't match passes the command to a matching installed toolchain, or stops with the one command that installs it. It never downloads anything on its own.
- **Every break comes with its migration:** a deprecation warning with a machine-applicable fix in a release before the removal, `k fix` to apply it, and `k upgrade` to move a project to a new toolchain.
- **Every lockfile and wire format carries a format version,** and two adjacent minors always understand each other on the wire, so a workspace's services can be deployed one at a time across a toolchain upgrade.
- **Changelog entries are fragments in `changes/`,** written in the pull request that makes the change and compiled into `CHANGELOG.md`, the release notes and the book's upgrade guide at release time.
- **The docs are versioned per minor** (`/0.5/`), with `/next/` for main and `/` for the newest release ([49-documentation.md](49-documentation.md) designs the site itself). Every page is built from the release's own commit, so its snippets compiled against that release's compiler. The site is the repository's GitHub Pages, `https://slavkobojanic.github.io/kobra/`, with the install scripts and `index.json` at its root; each release also attaches its book, installed with the toolchain and opened offline with `k docs` ([Public distribution](#public-distribution-decided-2026-11-23)).

Hosting (the repository, CI, the docs site, release binaries, the container registry, the extension marketplaces, package managers) is not decided here: each is an **owner decision**, listed at the end with the options. Nothing below needs any of them to be built and tested.

## What a version promises

### Semantic versioning

| Version change | Before 1.0 | From 1.0 |
|---|---|---|
| patch (`0.5.1 → 0.5.2`, `1.4.1 → 1.4.2`) | fixes only: no new error, no new warning, no removed API, no format change | the same |
| minor (`0.5 → 0.6`, `1.4 → 1.5`) | may break, with a migration (below) | additions, deprecations and new warnings; breaks only behind an edition |
| major (`1 → 2`) | n/a | reserved for a change editions can't express (a runtime or wire break that can't be bridged); not planned |

- **The compatibility promise:** a project that `k check` accepts with no problem on `X.Y.Z` is accepted, and behaves the same, on every later `X.Y.*`; and from 1.0 on every later `X.*` in the same edition.
- **Exceptions, in any release:** a soundness fix (the compiler accepted something the language rejects), a security fix, and a fix to behaviour the documentation already described differently. Each still needs a changelog fragment of kind `breaking`, and a diagnostic with a fix where one is possible.
- **Patch releases add no diagnostics,** errors or warnings. `k check` exits 1 on any problem, warnings included ([13-tooling.md](13-tooling.md#commands)), so a patch with a new warning would break every project's CI on a version its requirement accepts.

### What counts as breaking

A change is breaking when a project accepted with no problem stops being accepted, behaves differently, or can no longer read what the previous version wrote. By surface:

| Surface | Breaking | Not breaking |
|---|---|---|
| **Syntax** | removing or changing an accepted form; a new keyword (it can collide with a name) | a new form that was a syntax error before |
| **Type rules** | rejecting a program that was accepted; changing which overload or member resolves | accepting a program that was rejected; better inference that changes no accepted program's meaning |
| **Diagnostics** | a new error or warning on accepted code; reusing or removing a code; changing a fix's applicability from machine-applicable | a new code for code that was already rejected; changed message text |
| **Standard library** | removing, renaming or changing the signature of a public declaration; a new case in a public enum that user code matches; a changed default; a new name in the implicit imports that can collide | a new declaration in a module that has to be imported; a new member of a type users can't implement |
| **Runtime behaviour** | changing documented behaviour: rendering, events, reactivity order, error mapping, accessibility attributes, server startup, shutdown or exit codes | performance; generated JavaScript and CSS (internal, never API); undocumented DOM structure beyond the element mapping |
| **Lock and project files** | `k.toml` keys or meaning; a lockfile format change an older toolchain misreads | a new format version read by the new toolchain alongside the old one (below) |
| **Wire and stored formats** | the rpc wire format, the service token, streaming framing, the session cookie, the runtime's own tables (`k_session`, `k_job`, `k_rate_limit`, `k_migrations`) | a change that the previous and the next minor both understand (below) |
| **CLI** | removing or renaming a command or flag; changing an exit status; the shape of `--format json` diagnostics; the commands of `dist/app` and the variables it reads | human-readable output; new commands and flags |
| **Language server** | dropping a standard LSP feature an editor relies on | everything else: the server always comes from the project's own toolchain (below) |

- **Generated code is internal.** Tests and tools must not depend on emitted JavaScript, CSS class names or bundle layout; the documented API is the element mapping, the ARIA attributes, `dist/` paths and `dist/manifest.json`.
- **Diagnostic codes are API; messages aren't.** A code is never reused ([`k_diagnostics::codes`](../../crates/k_diagnostics/src/codes.rs)). The JSON diagnostic gets a `"schema": 1` field, and its shape changes only with that number.
- **Dependencies' warnings are not reported.** A git package's code is checked for errors, but its warnings belong to its author, so a new lint in a minor never fails a project because of a package it can't edit (Cargo's lint capping). Workspace libraries are the team's own code and are reported in full.

### Editions

**Kobra adopts editions at 1.0, not before.** An edition (`edition = "2027"` in `[package]`) is a set of syntax and type rules a package opts into; the toolchain supports every edition, and packages of different editions build together, because packages are distributed as source ([18-packages.md](18-packages.md)) and one build mixes them.

- **What an edition may change:** syntax, keywords, type rules, which diagnostics are errors, and which deprecated standard library names are visible. **What it can't:** the standard library's implementation, the runtime and the wire formats, which are one per toolchain and shared by every package of a build.
- **Before 1.0 each minor plays the edition's part,** and `k upgrade` (below) migrates. Running several rule sets in one compiler before the language has settled would cost more than it saves: there is no ecosystem of packages on older versions yet.
- `k new` writes the current edition; a `k.toml` without `edition` is the first edition.

### Stability levels

Every language feature and standard library declaration is at one of three levels:

| Level | Promise | How it is used |
|---|---|---|
| **stable** | the compatibility promise above | freely |
| **preview** | may change or disappear in any minor, with a changelog entry but no deprecation period | the project lists the feature in `[package] preview = ["client-streaming"]` |
| **internal** | none | not at all: used by generated code and the runtime only |

- **Preview is opt-in per project and available on every channel.** Unlike Rust's nightly-only features, a preview doesn't push anyone onto nightly; the opt-in in `k.toml` is the consent, and it is visible in review.
- **Consent is not inherited.** A package that uses a preview feature requires every project that depends on it to list that feature too (a proposed `K82xx` error naming the package, with a machine-applicable fix that adds it), so no project starts depending on unstable behaviour without saying so.
- **In the standard library a declaration is marked in source:** `@Preview("feature-name")` and `@Internal`. Using a preview declaration without the opt-in is an error with a fix that adds the opt-in; using an internal one is the existing visibility error.
- **The feature table** (`k_sema::features`) is the one list of named features that aren't simply stable: each row has an id (`client-streaming`, `k-new`), a status (`planned`, `preview` or `stable`), the version it changed status in, and its design topic. Language features are marked there; stdlib declarations point at a row by their `@Preview` name. The book's `> **Planned** (id).` and `> **Preview** (id).` notes use the same ids, and the book test fails when a note's id is missing from the table or its status disagrees ([49](49-documentation.md#planned-notes-are-tied-to-the-code)), so the docs and the compiler share one source of truth for what is built.
- **A stabilized feature left in `preview`** is a warning with a fix that removes it; an unknown name is `K8101`, like any malformed manifest value.
- **In the docs:** the generated standard library reference shows a *Preview* or *Deprecated* badge from the annotations, and a book chapter marks a preview feature with a `> **Preview.**` note naming the feature, as it marks **Planned** ones today. Internal declarations never appear.

#### Stability levels: decided while implementing (2026-11-12)

Built in `k_sema::stability` and `k_sema::features`; [release.md §12](../compiler/release.md#12-implementation-plan) step 7.

- **The first preview feature is `workers`** (`@Worker`, `WorkerFailure`, `WorkerOnlyError`, [55-workers.md](55-workers.md)), the newest built feature: its API is the most likely to change. `examples/notes` lists it.
- **K8108 `preview-not-enabled`** is reported at each use (the name token, or `@Name` for an annotation) with a machine-applicable fix that adds the id to `[package] preview`, creating the key after the section's last line. Imports and doc links aren't uses.
- **Consent is checked from the manifests:** K8109 `dependency-preview` is reported in the depending project's `k.toml`, at the library's `[dependencies]` line, for each preview the library's `preview` lists and the project's doesn't. It goes by the library's list, not by what its code happens to use, so the requirement is visible in the library's manifest.
- **A file without a `k.toml`** (loose files, the editor on a scratch file) may use every preview: there is no project to consent. The corpus's single-file cases list every preview in their implicit manifest.
- **Ids in `preview`:** unknown or planned is K8101 (a note lists the features in preview), stable is the warning K8110 `stabilized-preview` with a fix that removes it.
- **`@Deprecated`'s warning is K1401 `deprecated`.** Its fix is machine-applicable when `replacement` names a declaration of the same kind beside the deprecated one (a member of the same type, a declaration of the same module) with the same argument labels, and the use can see it without a new import (a member, a use in the same module, or a `k.core` declaration); otherwise a help line names the replacement. Uses inside the deprecated declaration itself aren't reported, nor uses in the standard library's own files.
- **`@Internal`** keeps a public standard-library declaration out of the prelude (the stdlib still sees it, like `k.internal`) and makes importing it from user code K1102; the reference leaves it out. No declaration is internal yet.
- **The badges:** the generated stdlib reference puts `> **Preview** (id).` (a book note the feature table check reads) and `> **Deprecated** since X.` on the declaration, and *Preview.* / *Deprecated.* in the module's summary table. The design-status page lists the preview rows in a generated table, as it lists the planned ones.
- **Dependencies' warnings** (Cargo's lint capping) wait for git packages; workspace libraries are reported in full, as above.

### Deprecation

- **`@Deprecated("message", since: "0.6", replacement: "newName(_:)")`** may be put on any declaration, the standard library's or a package's. Using it is a warning; when `replacement` names a declaration whose call shape matches, the warning has a machine-applicable fix.
- **Removal timing:** before 1.0, a stable declaration or form is deprecated for at least one minor before the minor that removes it (deprecated in 0.6, removed in 0.7 at the earliest). From 1.0, a standard library declaration is never removed; a new edition may hide it.
- **Removed syntax leaves a tombstone:** for one minor after removal, the parser still recognises the old form and reports it with the fix the deprecation had, instead of an unrelated syntax error. `k upgrade` relies on it.
- **Removed `k.toml` keys and CLI flags** get the same treatment: a precise error naming what replaces them, for one minor.

## Channels and cadence

| Channel | Version | Built from | Promise |
|---|---|---|---|
| **stable** | `0.6.0`, `0.6.1` | a tag on the release branch `release/0.6` | the compatibility promise |
| release candidate | `0.6.0-rc.1` | the release branch, one week before the stable minor | a stable candidate; installed only by asking for it |
| **nightly** | `0.7.0-nightly.20261109` | main, every day it changed, after the nightly gate | none: what main is |

- **No separate beta channel.** Release candidates serve that purpose without a third stream to maintain; a beta can be added at 1.0 if users ask for one.
- **Cadence:** a minor every six weeks once the first release is public (train model: what is on main and green at the branch cut ships; nothing waits for a feature). Patches whenever a fix is needed, from the release branch. Before the first public release, minors are cut when the owner decides.
- **Only the latest stable minor gets patches.** A security fix may also be backported to the previous minor.
- **A nightly satisfies its own minor's requirement:** `0.7.0-nightly.…` runs a project requiring `k = "0.7"`, but not one requiring `0.6`, since it may already have broken 0.6 code.
- **`k --version`** prints the version, the commit and its date: `k 0.6.1 (3fa1c2e 2026-11-02)`; a nightly says `nightly`. `dist/app --version`, its startup log line and `dist/manifest.json` carry the toolchain version that built it, so an operator can tell which runtime is deployed.

## Pinning a toolchain

### The requirement and the lock

- **`[package] k = "0.5"` is a requirement, with Cargo's caret meaning:** `0.5` is `>=0.5.0, <0.6.0`; from 1.0, `1.4` is `>=1.4.0, <2.0.0`. `=0.5.2` pins exactly. It replaces today's "minimum toolchain version" reading ([24-projects.md](24-projects.md#ktoml)), which accepts every later minor even though before 1.0 any later minor may break the project.
- **`k.lock` records `toolchain = "0.5.2"`,** the exact version that last wrote it, in apps and at a workspace root. Libraries record none: their consumers' toolchains build them. So a team and its CI build with the same compiler, and a bump shows up in review as a one-line diff.
- **A workspace has one toolchain.** The root's `k.toml` gets the requirement (`[workspace] k = "0.5"`), and a member whose `k` is incompatible with it is an error; members needn't repeat it.
- **A git package that requires a newer toolchain** than the one building is an error naming the package and its requirement.

### When the installed `k` doesn't match

Toolchains live in `~/.k/toolchains/<version>/` and the `k` on the `PATH` is the default one (installed in `~/.k/bin/` by the install script, below). Every command that works on a project, `k lsp` included, starts by reading the requirement and the lock:

1. **The running `k` satisfies the requirement and equals the locked version:** it runs.
2. **Otherwise, an installed toolchain is the locked version** (or, without a lock, the newest installed one that satisfies the requirement): the running `k` executes it with the same arguments, environment and standard streams. It sets `K_TOOLCHAIN_DELEGATED=1`, and a delegated `k` never delegates again.
3. **Otherwise it stops** (exit 2) with one diagnostic (`K8201 toolchain-mismatch`): *"this project needs k 0.5 (k.lock: 0.5.2); this is k 0.6.1"*, with the commands that resolve it: `k toolchain install` (what the project needs) or `k upgrade` (move the project to this toolchain). It is a diagnostic like any other, so `--format json` gives agents the exact command.
4. **A satisfying `k` that differs from the lock only in patch** runs, and `k build` updates the lock's `toolchain`. Patches never break, so refusing would only cost time; the lock diff still shows the change.

- **`K_TOOLCHAIN=0.5.2` overrides** the choice for one command, for bisecting and for trying a release candidate (`K_TOOLCHAIN=0.6.0-rc.1 k check`).
- **No implicit downloads.** Go downloads a missing toolchain automatically; Kobra doesn't: a command such as `k check` on a fresh checkout must not reach the network, CI should say what it installs, and an offline machine should fail with a clear message rather than a timeout. The fix is one command, and `k toolchain install` without arguments installs exactly what `k.lock` names.
- **The language server follows the same rule,** so an editor uses the project's own compiler and the editor never disagrees with that project's CI. The VS Code extension needs no version logic of its own (below).
- Delegation exists from the release that introduces it; an older default `k` can't delegate to a newer toolchain, so the install script always installs the newest stable as the default.

#### Decided while implementing (2026-11-12)

Built in `k_sema::workspace` (requirements) and `k_cli`'s `toolchain` module (the choice); [release.md §12](../compiler/release.md#12-implementation-plan) steps 2 and 3.

- **Requirement syntax:** one to three numbers (`0`, `0.5`, `0.5.2`), optionally after `=`. No ranges, wildcards, `^` or `~`; anything else is `K8101`. A pre-release is written only exactly (`=0.6.0-rc.1`), and **a pre-release satisfies a caret requirement when its release does**, so `0.7.0-nightly.20261109` and `0.7.0-rc.1` satisfy `0.7` and nothing else, and `K_TOOLCHAIN=0.6.0-rc.1` works on a project requiring `0.6`.
- **A member may omit `k`** when the root has `[workspace] k`; without one, each member needs its own. A member's requirement may narrow the root's (`0.5.2` under `0.5`); one that shares no version with it is `K8107 conflicting-toolchain`. The toolchain must satisfy the root's and every member's requirement.
- **Without a lock, delegation chooses the newest installed *release*** that satisfies the requirements; a pre-release is used only when the lock or `K_TOOLCHAIN` names it.
- **A lock the requirements no longer allow is stale** (someone raised `k` by hand): it is ignored, as if there were none, and the next `k build` rewrites it.
- **`K_TOOLCHAIN` wins over the requirement and the lock** and applies to every command, inside a project or not, `k --version` included, so bisecting needs nothing else. Naming the running version runs it; a value that isn't a version is a usage error (exit 2); an uninstalled one is `K8201` naming its install command.
- **Toolchains live in `$K_HOME/toolchains/<version>/k`,** with `K_HOME` defaulting to `~/.k`; the executable keeps the running one's file name. Tests point `K_HOME` at a directory of fakes.
- **Delegation replaces the process on Unix** (`exec`), so signals, the exit status and an editor's pipes to `k lsp` reach the chosen toolchain unchanged; elsewhere the running `k` waits for it and returns its status.
- **`K8201` points at `k.lock`'s `toolchain` line** when the lock chose the version, else at the first requirement's line. Its help names `k toolchain install <locked version>` (plain `k toolchain install` without a lock) and `k upgrade`; a note lists what is installed. An unreadable `k.lock` stops the command with `K8002`.
- **`k build` writes `k.lock`** in an app's directory or at the workspace root, never in a member or a library, and only when the running toolchain satisfies the requirements (so a `K_TOOLCHAIN` experiment doesn't rewrite the pin). `k check` never writes it, and `k new` writes none: the first build does. The lock keeps everything else it holds (packages, later), replacing only its `toolchain` line.

### Commands

| Command | Does |
|---|---|
| `k toolchain install [version]` | installs a toolchain (default: the one the project's `k.lock`, else its requirement, names; `stable`, `nightly` and `0.6.0-rc.1` also work), verifying its checksum and signature |
| `k toolchain list` | the installed toolchains, the default, and which one the current project uses |
| `k toolchain remove <version>` | removes one |
| `k toolchain default <version>` | makes one the `k` on the `PATH` |
| `k self update` | installs the newest stable and makes it the default; on nightly, the newest nightly |

Downloads go to the release host chosen by the owner (below) through an index file listing each version's archives, sizes, SHA-256 digests and signatures; `K_TOOLCHAIN_INDEX` points it elsewhere, which is how the tests serve a fake release from a local directory.

#### Commands: decided while implementing (2026-11-12)

Built in `k_cli`'s `toolchains`, `index` and `archive` modules, with `k_server::outbound::download`; [release.md §12](../compiler/release.md#12-implementation-plan) step 4.

- **The index** is `index.json`: `{"format": 1, "releases": [{"version", "date"?, "yanked"?, "archives": [{"target", "url", "size"?, "sha256"}]}]}`. A release's channel follows from its version (none, `-rc.N`, `-nightly.<date>`); `yanked` holds the reason; an archive's `url` is absolute or relative to the index's URL, so one fake release directory serves the index and the archives. Unknown keys are ignored (a `signature` joins the archives when signing is wired in, without a new format); a newer `format` is refused with "update k with the install script".
- **Only release builds have a built-in index.** *(Superseded on 2026-11-23 by [Private distribution](#private-distribution-decided-2026-11-23): every `k` reads the repository's releases through GitHub's API (since [Public distribution](#public-distribution-decided-2026-11-23), without a token unless one is found), and `K_DEFAULT_TOOLCHAIN_INDEX` is gone.)* A release build got its index URL at build time (`K_DEFAULT_TOOLCHAIN_INDEX`), the docs site's `index.json`; a `k` without one said to set `K_TOOLCHAIN_INDEX` and downloaded nothing.
- **What `install` chooses:** a version (`0.3.1`, `=0.3.1`, `0.4.0-rc.1`) exactly, yanked or not (with a warning); a minor or other requirement (`0.3`), `stable` or `nightly`, the newest release of it that isn't yanked and has an archive for this target (never a pre-release for a requirement or `stable`); without an argument, `k.lock`'s toolchain, else the newest release every requirement allows, else outside a project the newest stable. A choice that only a yanked release would fit names it and says it installs only when named exactly. An installed version is left as it is.
- **Atomic install:** the archive downloads into `$K_HOME/tmp/install-<version>-…/`, its size and SHA-256 are checked against the index while it streams, it unpacks there, and only then is the directory renamed into `toolchains/<version>/`. A failure removes the staging directory; a killed install leaves only that directory, which a later install removes once it is an hour old, and nothing in `toolchains/`. `installed()` reads only `toolchains/`, so delegation never sees half an install.
- **Archives** hold `k-<version>-<target>/` with `k` (`k.exe`), `README.md`, `LICENSE-MIT` and `LICENSE-APACHE`; the files may also be at the root. Only regular files and directories are unpacked: a link, an absolute path or `..` is refused, and macOS metadata (`._*`, `__MACOSX/`) is skipped. The tar and zip readers are `k_cli`'s own, about 200 lines over `flate2`, rather than two archive crates.
- **The default** is a copy of its `k` in `$K_HOME/bin/` (put there by a rename, and on Windows after moving the running one aside), with its version in `$K_HOME/default`. A copy rather than a link works the same on every system and keeps working however the toolchains directory changes. `remove` refuses the default; `default` takes an installed exact version; `install` never changes the default, but says how to set one when there is none.
- **`list`** prints one installed version per line, oldest first, tagged `default`, `this project` (the toolchain delegation would run for the working directory's project) and `yanked: <reason>`; when the project's toolchain is this `k` or isn't installed, a last line says so.
- **Yanked notes need no network:** every fetch keeps the index in `$K_HOME/index.json`; `list` and `K8201`'s note on the installed toolchains read that copy.
- **`k self update`** installs the newest stable (the newest nightly when the running `k` is a nightly; a release candidate counts as stable) and makes it the default; when it already is, it says so and changes nothing.
- **`k upgrade` still never downloads.** It names `k toolchain install <minor>` for each missing toolchain and stops before changing anything: `k toolchain install` is the one command that reaches the network, which keeps "no implicit downloads" a rule without exceptions.
- **Exit status:** 0 on success, 1 when the command fails (no index, a network failure, a mismatch, nothing fits, not installed), 2 for an argument that isn't a version.

## Upgrading a project

- **`k fix`** applies every machine-applicable fix of the current diagnostics, then re-checks, until nothing changes (at most a few rounds). `--code K1234` limits it to one code and `--dry-run` prints the diff. It is useful on its own: agents already consume these fixes from JSON, and this is the same thing in one command.
- **`k upgrade [--to 0.7]`** moves a project to the running toolchain (or the given one):
  1. it refuses with uncommitted changes, unless `--allow-dirty`, so the upgrade is one reviewable diff;
  2. it applies the deprecation fixes of each minor in between, in order. Removed forms are recognised by their tombstones for one minor, so from more than one minor behind it runs each intermediate minor's newest installed patch in turn (it names the `k toolchain install` commands for the missing ones rather than downloading them; see "Commands: decided while implementing");
  3. it rewrites each lockfile to the current format, and changes `k = "…"` and `k.lock`'s `toolchain`;
  4. it runs `k check` and prints what it couldn't fix, each problem with the upgrade guide's section for it.
- **From 1.0, `k upgrade --edition 2030`** does the same for an edition, using the edition's migration lints.
- **A breaking change without a machine-applicable fix** is allowed only with a manual migration written in the upgrade guide, which the changelog fragment links (below).

#### `k fix`: decided while implementing (2026-11-12)

Built in `k_diagnostics::fix` and `k_cli`'s `fix` module; [release.md §12](../compiler/release.md#12-implementation-plan) step 6.

- **A round applies the first machine-applicable fix of each diagnostic,** in report order. A fix whose edits overlap one already picked (or that inserts at the same point) waits for the next round, computed against the new text; a fix two diagnostics ask for is applied once. Maybe-incorrect fixes are never applied.
- **At most 8 rounds.** A pair of fixes that keep undoing each other stops there, with a warning and exit status 1, instead of looping.
- **It writes only the checked group's own files:** a fix in a workspace library is made in the library's file, once, and nothing in the standard library is ever touched. It never runs tests.
- **Its output is `k check`'s:** the problems that remain after the fixes, human or `--format json`, then `fixed N problems in M files: …`. It exits 1 when any problem remains, so it can gate a commit. `--dry-run` prints a unified diff on standard output and writes nothing.
- **The corpus pins every fix's result:** each `sema` case with a machine-applicable fix has a `.fixed` file made by the same loop, and each fix alone must lower its code's count on a re-check.

#### `k upgrade`: decided while implementing (2026-11-12)

Built in `k_cli`'s `upgrade` module, with `k_sema::lockformat::upgrade` and `k_diagnostics::codes::MIGRATIONS`; [release.md §12](../compiler/release.md#12-implementation-plan) step 8.

- **Where the project is:** the version `k.lock` records, else the lowest version its requirements allow (`0.5.0` for `k = "0.5"`). **Where it goes:** the running toolchain, or `--to`, which takes a version or a minor (its newest installed release). A target older than the project is refused: `k upgrade` never moves a project back.
- **"Uncommitted changes" means `git status --porcelain` lists something below the project's root**, untracked files included. A project outside a git repository is refused too, like Cargo's `cargo fix`; `--allow-dirty` covers both.
- **The minors in between run in order on their newest installed release** (never a pre-release; this toolchain for its own minor), each as `k upgrade --to <version> --allow-dirty` with `K_TOOLCHAIN_DELEGATED=1`, so each toolchain does its own part with its own tombstones. Every one must be installed before anything changes; a missing one is named with its `k toolchain install` command and nothing is touched. A step that leaves problems stops the chain at that minor, since the next minor may no longer recognise what remains. More than one step prints the rolling-deploy warning.
- **Only the migration codes' fixes are applied:** `k_diagnostics::codes::MIGRATIONS` (`K1401 deprecated`, `K8110 stabilized-preview`; each tombstone's code joins it), so the upgrade's diff holds only the migration and other quick fixes stay `k fix`'s. A changelog fragment's `migration:` must name one of these codes.
- **Requirements:** each `k` the target doesn't satisfy, or whose lowest version is of an older minor, becomes the target's minor (`0.6`), or `=` the target's version for an exact requirement, keeping the line's formatting. `k.lock`'s `toolchain` is rewritten when the lock exists; otherwise the next `k build` writes it.
- **Lockfiles:** every lockfile below the root (hidden directories, `dist`, `target` and `node_modules` skipped) goes through `lockformat::upgrade`, which applies each format change's conversion (`lockformat::STEPS`, none yet) and writes `format = N`; a lock already in the current format is left byte for byte. A lock too new to read (`K8202`) stops the upgrade before anything is written.
- **Everything is computed in memory, then written at once:** lockfiles, requirements and the lock first, then the fixes over those texts, then one write; a failure before the write leaves the project as it was.
- **What remains** is the fix loop's last check (as `k check --no-tests`), human or `--format json`; each remaining problem whose code a generated "Upgrading to X.Y" section of the upgrade guide names, for a minor the upgrade crossed, is followed by a note naming that section. `k` embeds the guide of its own release. The exit status is 1 when anything remains.

### Lockfile format versions

- **Every lockfile starts with `format = N`** after its comment header: `rpc.lock`, `schema.lock`, `storage.lock`, `routes.lock` and `k.lock`. A lockfile without the key is format 1, which is every lockfile written so far.
- **A toolchain reads its own format and every older one,** and writes the format it read. Only `k upgrade` rewrites a lock to a newer format, together with the requirement bump, so a teammate's older toolchain is stopped by the requirement with a clear message instead of misreading the lock.
- **A lockfile in a newer format than the toolchain knows** is an error (`K8202 lockfile-too-new`) that names the format and the toolchain needed, never a parse error.
- **Every format change has golden fixtures:** the lock of each format under `tests/compat/locks/<file>/<format>/`, with a test that the current toolchain reads every one and that `k upgrade` turns each into the current format.

#### Decided while implementing (2026-11-12)

Built in `k_sema::lockformat`, used by every lockfile's reader and writer; [release.md §12](../compiler/release.md#12-implementation-plan) step 5.

- **Each lockfile has its own format number** (`k_sema::lockformat::FORMATS`; `schema.lock`'s is also `k_db::lock::FORMAT`, and a test keeps the two equal). All five are at format 1.
- **Format 1 is written without the key,** so the locks of 0.1 are byte for byte what every earlier toolchain wrote, and nothing in the examples changed. A lock that says `format = 1` keeps saying it when rewritten; from format 2 the writer always writes the key, after the comment header. "Every lockfile starts with `format = N`" is therefore true from the first format change.
- **`format` must be a positive whole number;** anything else is `K8002`. A number above the toolchain's is `K8202` at the `format` line, reported instead of anything else about that file: the lock isn't compared with the program and `k build` doesn't rewrite it. For `k.lock` the driver reports it before the command, like `K8201`, and a `K_TOOLCHAIN` build leaves such a lock as it is.
- **`K8202`'s help** says a newer toolchain wrote the lock, names `k toolchain install` and the toolchain `k.lock` records, and suggests restoring the file from version control.
- **The goldens describe one fixture program** (`tests/compat/locks/project/`), and each format's lock of each file sits under `tests/compat/locks/<file>/<format>/<file>`. The compat suite checks one golden per format up to the current one, that each is read without a problem and written back unchanged (with and without a declared `format = 1`, and when stale), that a newer format is `K8202` and never rewritten, and that `k upgrade`'s conversion (`lockformat::upgrade`) turns every golden into the newest one and leaves the current format as it is.

### Wire and stored formats across a rolling deploy

A workspace's apps are deployed one at a time, and open browser tabs keep running the previous client: during a deploy, code built by two toolchains talks to each other. `rpc.lock` already protects the *protocols' shapes* ([40-workspaces-and-services.md](40-workspaces-and-services.md#compatibility-between-deploys)); this protects the *toolchain's encodings* of them.

- **Any two adjacent minors interoperate, in both directions.** A change to an encoding takes two minors: release N learns to read the new form and keeps writing the old; release N+1 writes the new. So a deploy may mix N and N+1, never N and N+2.
- **This covers** the rpc wire format and its error details ([28-rpc-wire-format.md](28-rpc-wire-format.md)), the service token (already versioned, `v1.`), server-stream framing ([29-server-streaming.md](29-server-streaming.md)), the session cookie, job payloads in `k_job` (a job queued by N runs on N+1, and the other way round), the `k-data` payload a page embeds, and the binary Protobuf encoding when it ships.
- **The runtime's own tables migrate by expand and contract,** exactly as an app's must ([19-persistence.md](19-persistence.md#rolling-deploys-expand-then-contract)): a toolchain migration that adds a column ships in N, and the one that drops what it replaces in N+1 or later. `dist/app` applies them like the app's migrations, under the same advisory lock.
- **The release notes say** when a minor starts writing a new encoding, so operators know not to skip a minor in one deploy. `k upgrade` from more than one minor behind prints the same warning.
- **Tested with recorded traffic:** `tests/compat/wire/<minor>/` holds requests and responses recorded from each minor's examples (rpcs, error details, a stream, a service token, a queued job row). The current server must answer the previous minor's recorded requests as recorded, and the previous minor's recorded responses must decode in the current client. Fixtures are offline and need no old binary.

#### Decided while implementing (2026-11-12)

Built as the `compat::wire` tests of `k_server` ([release.md §12](../compiler/release.md#12-implementation-plan) step 10); the recordings are `tests/compat/wire/0.1/`.

- **A recording is a transcript, one JSON file per example:** each step's title, the request sent, and what came back: the status and the headers that belong to the format (`content-type`, `cache-control`, `set-cookie`), the body, a stream's envelopes, a stored row, a call the server made, or what the browser client decoded. `paybox` (from `examples/shop/paybox`) covers service-token calls, a declared error with a payload, `rpcFailure` for a call without a token and for an invalid message, the `k_job` row a charge queues and the delivery that row makes; `notes` covers the session cookie, `rpcFailure` for a guest, a server stream's envelopes, a declared error and a page's `k-data`.
- **Replaying sends the recorded requests** to the current build of the same example and compares what comes back step by step. Values that differ on every run (ids, instants, ports, cookie values, a token's expiry and MAC) are compared by their shape, and an id first seen in a recorded response is mapped to this run's, so a later recorded request that names it still means the same row. A recorded service token is signed again for now with the recorded version and caller.
- **"As recorded" allows additions:** every recorded field must be there with the same value, and a response may carry fields the recording doesn't (an optional field added compatibly). Dropping or changing a field, a status, a header or an envelope fails.
- **The stored row is replayed too:** the recorded `k_job` payload replaces the one the current build queued before the current worker runs it, so a job queued by the recorded minor runs on the current one.
- **The client side runs the current browser client** (the example's generated client chunks in V8 with the fake DOM and a `fetch` that answers the recorded response): each recorded rpc response is decoded by the stub the app calls, the stream's recorded envelopes by its stream reader, and the recorded `k-data` starts the app, which must stay on the signed-in page and render the note from the recorded stream.
- **Only the current minor is recorded** (`K_BLESS=1`); older minors' transcripts are never rewritten. Until 0.2 is cut, a change to the examples that alters a recorded answer re-records 0.1.
- **The shop's service client** decoding paybox's recorded responses isn't driven separately: the remote stubs use the same generated codecs as the browser's, which the notes transcript exercises.

## The release process

The maintainer runbook, step by step, is [docs/compiler/release.md](../compiler/release.md). The decisions it follows:

### One source for the version

- **The workspace `Cargo.toml` `version` is the version.** Everything else that names it is derived or checked against it: `editors/vscode/package.json`, the `k` requirement that `k new` writes, the image tags in [38-deployment.md](38-deployment.md) and the examples' Dockerfiles, the book's install page, and the examples' `k = "…"`.
- **A test fails when any of them disagrees** (`crates/k_cli/tests/version.rs`), so a release can't ship half-bumped.
- **Release automation is Rust, in an `xtask` crate** (`cargo xtask release …`), not shell scripts or npm, like the rest of the toolchain. It bumps, compiles the changelog, checks the gate's local parts and names the release workflow to start, which makes the tag ([CI on request](#ci-on-request-decided-2026-11-23)).

### Changelog fragments

This section owns the fragment format that [49-documentation.md](49-documentation.md#docs-ownership-in-review) refers to.

**Each user-visible change adds a file to `changes/`** in the pull request that makes it, and the release compiles them. Conventional commits were rejected: this repository's commits each cover several topics ("Cancel work on unmount, reject more mistakes at compile time, and speed up re-checks"), and a commit subject is the wrong place for a migration note. A fragment is reviewed with the code it describes.

```markdown
---
kind: breaking            # breaking | language | stdlib | tooling | runtime | fix | deprecation | preview | docs | internal
design: 41                # optional: the design file
migration: K3108          # breaking and deprecation: the code whose fix migrates, or "manual"
book: apps/drag-and-drop.md   # the book pages changed in this PR, or "none: <reason>"; required for user-visible kinds
---
`Draggable` takes its kind first: `Draggable(kind:payload:)`. `k fix` reorders existing calls.
```

- **File name:** a short slug (`changes/draggable-kind-first.md`), so parallel pull requests never conflict.
- **Checked by a test:** every fragment parses, its kind is known, a `breaking` or `deprecation` fragment names a migration, a `manual` migration has a section in the upgrade guide, and every `book:` page exists.
- **The `book:` field is required on the user-visible kinds** (`breaking`, `deprecation`, `language`, `stdlib`, `runtime`, `tooling`, `preview`): a list of pages, or `none: <reason>`. It is optional on `fix` and absent on `docs` and `internal`. `cargo xtask docs-gate <base>` checks that each listed page changed in the same diff ([49](49-documentation.md#docs-ownership-in-review)); this is the review-time half of keeping the book current, the tests being the mechanical half.
- **The change checks require a fragment** (`ci.yml`, started on a pull request's branch: [CI on request](#ci-on-request-decided-2026-11-23)) in every pull request that changes `crates/`, `editors/` or `docs/book/`, unless it adds a `kind: internal` one; internal fragments are deleted, not published.
- **At release,** `cargo xtask release prepare` turns the fragments into a `CHANGELOG.md` section grouped by kind (breaking first), the release notes (that section plus `changes/_highlights.md`, written by hand), the book's changelog page, and a new section of the book's upgrade guide (the breaking and deprecation fragments with their migrations), then deletes them. [49-documentation.md](49-documentation.md#docs-ownership-in-review) relies on the same fragments: a change to the compiler's sources that doesn't touch the book must add one.

#### Changelog fragments: decided while implementing (2026-11-12)

Built in the `xtask` crate (`crates/xtask`, run as `cargo xtask`); [release.md §12](../compiler/release.md#12-implementation-plan) step 9.

- **`xtask` lives at `crates/xtask`**, like every other crate, so the workspace's `crates/*` members and the affected-tests rules cover it; `cargo xtask` is an alias in `.cargo/config.toml`. Its only dependencies are `k_diagnostics` (to check `migration:` codes) and `clap`.
- **Front matter** is `key: value` lines between `---` lines, with ` # comments`; `book:` is a comma-separated list or `none: <reason>`. Unknown keys are errors. `book:` is rejected on `docs` and `internal` fragments, optional on `fix`.
- **`migration:` names a code `k upgrade` applies** (one of `k_diagnostics::codes::MIGRATIONS`), or `manual`. A manual migration's section in the upgrade guide carries `<a id="<slug>"></a>`, which the test looks for and the generated entries link to.
- **The highlights are part of the `CHANGELOG.md` section**, under "Highlights" before the kinds, so the release notes the pipeline takes from `CHANGELOG.md` have them. Comment lines in `_highlights.md` are left out.
- **A release candidate's section is replaced by its release's:** a section is headed `## 0.6.0 (2026-11-08)`, or `## 0.6.0-rc.1 (pre-release of 0.6.0, <date>)`, and preparing any version of a release replaces that release's earlier section. In the upgrade guide each release's entries follow a `<!-- release 0.6.0 -->` line in its minor's `## Upgrading to 0.6` section, so they are replaced the same way; a minor's first release always gets its section ("Nothing to migrate" when empty), a patch with nothing to migrate adds nothing. Each entry names its migration's code, which is what `k upgrade` matches.
- **Links:** entries link the fragment's `book:` pages by their titles; `CHANGELOG.md` links into `docs/book/src/`, and the book's changelog page is its sections with those links made relative (a test keeps the copy current).
- **The docs gate** compares against the merge base with the given ref, with uncommitted and untracked files counted, so it can be run before committing. A fragment that a release deleted isn't an addition.
- **The local gate** (`cargo xtask release check`) is formatting, clippy with warnings denied, `cargo test --workspace`, the compat suite, the real-Chrome suites with `K_REQUIRE_CHROME=1` and `mdbook build`, after checking every fragment; `--release` adds "no fragment may remain". The nightly-only rows (fuzzing, long property runs, performance budgets) aren't repeated.
- **Seeding 0.1.0:** the first release's fragments are one per area of the toolchain built before fragments existed, written from the history, not one per commit.

### Artifacts

| Artifact | Name | Notes |
|---|---|---|
| toolchain archives | `k-0.6.0-<target>.tar.gz` (`.zip` on Windows), holding `k-0.6.0-<target>/` with `k`, `README.md`, `LICENSE-MIT`, `LICENSE-APACHE` | the six targets below; on GitHub Releases of `slavkobojanic/kobra`. `cargo xtask dist archive` refuses to pack without both licence files |
| checksums | `SHA256SUMS` | one line per archive |
| signatures | a Sigstore bundle per archive, and build provenance | keyless signing from CI; see owner decisions |
| index | `index.json` | versions, archives, digests, books and signatures, for `k toolchain`; attached whole to every release and nightly (the newest one's is the current index) |
| install scripts | `install.sh`, `install.ps1` | in the repository (`scripts/`), tested; served at the docs site's root and piped from there (`curl -fsSL https://slavkobojanic.github.io/kobra/install.sh \| sh`, `irm https://slavkobojanic.github.io/kobra/install.ps1 \| iex`; [Public distribution](#public-distribution-decided-2026-11-23)). They download the public release files with curl (or wget, or `Invoke-WebRequest`), with no token, install into `~/.k` with the layout `k toolchain` uses, make the toolchain the default and unpack its book; they print the `PATH` line rather than editing a shell profile, and refuse musl Linux by name. With no stable release published, they install the newest release candidate from the site's `index.json`, else the newest release's (else the newest nightly), and say so |
| VS Code extension | `kobra-0.6.0.vsix` | attached to every release; marketplaces are an owner decision |
| JetBrains plugin | `kobra-intellij-0.6.0.zip` | attached to every release, installed from disk ([13-tooling.md](13-tooling.md#jetbrains-ides)); the JetBrains Marketplace is an owner decision |
| toolchain image | `ghcr.io/slavkobojanic/kobra:0.6.0`, `:0.6`, `:latest`, `:nightly` | multi-arch (amd64, arm64), `debian:bookworm-slim` base (decided while implementing, below) |
| docs | `docs-0.6.0.tar.gz` on the release, unpacked into `~/.k/docs/0.6.0/` and opened with `k docs`; `/0.6/` on the docs site once the repository is public | built from the tag |

**Targets:** `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`, `x86_64-apple-darwin`, `aarch64-apple-darwin`, `x86_64-pc-windows-msvc`, and `aarch64-pc-windows-msvc` when the V8 build `deno_core` uses publishes a prebuilt library for it (until then that target isn't offered). CI builds the Linux and Windows targets, the owner's Mac builds `aarch64-apple-darwin`, and `x86_64-apple-darwin` isn't offered ([Private distribution](#private-distribution-decided-2026-11-23); kept when the repository went public, [Public distribution](#public-distribution-decided-2026-11-23)).

- **Each target is built and smoke-tested on a native machine** (a CI runner, or the owner's Mac for macOS) rather than cross-compiled: V8 is linked from a prebuilt static library per target, and a binary that was never run on its target isn't released. The smoke test is `k new`, `k check`, `k test`, `k build`, then `dist/app probe` against the built app's health page.
- **The Linux binaries link glibc** (V8 needs it, [38-deployment.md](38-deployment.md#what-k-build-produces)) and are built against an old glibc baseline (2.31), so they run on every current distribution. "Static binary" in the docs means *no other files and no Node*, not *statically linked*; musl isn't supported.
- **`dist/app` is a copy of `k`** ([architecture.md §14](../compiler/architecture.md#14-projects-commands-and-build-outputs)), so an app built on macOS runs only on macOS. Production images are built on Linux, which is what the toolchain image is for. A later `k build --target <triple>` can fetch that target's `k` through `k toolchain` and append the app to it; that is not part of this plan.
- **Container images contain only `k`** and CA certificates; the runtime image of a deployment is the app's own (`distroless/cc` plus `dist/app`, [38-deployment.md](38-deployment.md#the-toolchain-image)), so no Kobra runtime image is published.
- **Package managers** (Homebrew, winget, Scoop, the AUR, Nix, crates.io) are options for the owner; each would install the same archives and verify the same digests. Until one is chosen, the install script and `k toolchain` are the supported ways.

### The VS Code extension

- **The extension carries no compiler.** It starts `k lsp` in the workspace folder, and the `k` on the `PATH` delegates to the project's toolchain, so every language feature always matches the project's compiler, whatever extension version is installed.
- **Its version is the toolchain's,** and it is built on every release from the same commit. What it does own, the TextMate grammar and the language configuration, must colour the code of every supported toolchain, so a new keyword is added to the grammar when it lands on main, and a removed one stays in it until its tombstone is gone.
- **It reports the server's version** from the LSP `initialize` response (`serverInfo.version`) in its status item, so a user can see which toolchain a project uses.
- **Nightly builds** of the extension use the marketplace's pre-release flag when it is published there, with the date as the patch number (`0.7.20261109`), because the marketplace accepts only `major.minor.patch`.

### The gate

A release is made only from a commit on which all of these passed. The pipeline below implements it; GitHub Actions is the default, and the CI system is an **owner decision**.

| Check | Command | Change checks (`ci.yml`) | Nightly | Release |
|---|---|---|---|---|
| formatting and lints | `cargo fmt --all --check`, `cargo clippy --workspace --all-targets` | yes | yes | yes |
| Kobra formatting | `k fmt --check examples` ([56-formatter.md](56-formatter.md#the-command)) | yes | yes | yes |
| full test suite, which includes the book's snippets (`k_sema/tests/book.rs`), the generated references (`k_corpus/tests/book_reference.rs`) and the examples' check, build and tests | `cargo test --workspace` | Linux | every CI target (Linux, Windows) | every CI target; macOS on the owner's Mac |
| on Postgres | `K_TEST_POSTGRES_URL=… cargo test -p k_db -p k_server` | Linux | Linux | Linux |
| real Chrome | `K_REQUIRE_CHROME=1 cargo test -p k_cli --test it -- chrome:: dist_browser::` | Linux | Linux | Linux; macOS on the owner's Mac |
| long property runs | `PROPTEST_CASES=…` on the property suites | no | yes | the nightly's result for this commit, or a run |
| fuzzing | each target of `fuzz/` for 10 minutes; the committed corpus replayed at release | no | yes | corpus replay |
| performance budgets | `scripts/bench-check.sh` | no | on the dedicated runner | on the dedicated runner |
| the book builds with no warnings | `mdbook build docs/book` | yes | yes | yes |
| docs freshness | below | yes | yes | yes |
| docs ownership | `cargo xtask docs-gate <base>`: each user-visible fragment's `book:` pages are in the diff | yes | — | — |
| the versioned site builds | `cargo xtask docs build --version <minor>` (substitution, redirects, `versions.json`, search index) | when `docs/book/` or `xtask/` changed | yes (`/next/`) | yes |
| compatibility fixtures | `cargo test -p k_server --test it -- compat::` (locks, wire) | yes | yes | yes |
| version consistency, changelog fragments | `crates/k_cli/tests/version.rs`, the fragment test; a fragment per pull request | yes | — | yes, and no fragment may remain |
| install scripts | against a fake release served locally | when they change | yes | yes |
| artifacts' smoke test | per target, above | no | yes | yes |
| toolchain image | build, then build `examples/shop` with it (`K_TOOLCHAIN=` the image) | no | yes | yes |

- **Performance budgets need a quiet machine.** Shared CI runners vary by more than the budgets' margins, so `bench-check.sh` runs on one dedicated runner whose baseline is in [docs/compiler/performance.md](../compiler/performance.md); where it runs is an owner decision. Without one, the release manager runs it on a known machine and pastes its output into the release checklist.
- **Nightly failures block the train:** the branch cut waits until the nightly of the commit to be cut is green.
- **Which change checks are required** before merging is an owner decision (below); the recommendation is every "Change checks" row, so a docs test failing blocks the merge, not only the release. Since CI runs only on request, a required check is one a reviewer starts on the pull request's branch ([CI on request](#ci-on-request-decided-2026-11-23)).

#### The pipeline: decided while implementing (2026-11-12)

The workflows are [release.md §9](../compiler/release.md#9-the-pipeline); `crates/xtask/tests/workflows.rs` reads them back and requires every row of the table above in the workflows its columns name.

- **Every job is off until the owner switches it on.** Each job's `if:` starts with `vars.K_CI_ENABLED == 'true'`, a repository variable, so the workflows can be pushed to the private repository and run nothing. Deploying the docs also needs `K_PAGES_ENABLED`, and anything that publishes (the nightly pre-release, pushed images, the `publish-*` jobs) `K_PUBLISH_ENABLED`, so switching tests on never publishes. The performance jobs run on the runner labelled by `K_PERF_RUNNER` and are skipped without one; the draft release then waits for the rest, and the release manager runs `bench-check.sh` by hand. *(Superseded 2026-11-23 by [CI on request](#ci-on-request-decided-2026-11-23): no event starts a workflow, so `K_CI_ENABLED` is gone and `K_PUBLISH_ENABLED` guards only the image pushes; `K_PAGES_ENABLED` and `K_PERF_RUNNER` are unchanged.)*
- **`publish.yml` is part of `release.yml`** (its `release: published` jobs): one file holds the whole release, and one test covers it. *(Superseded 2026-11-23: with no `release: published` event, publishing is `publish.yml` again, started by hand; the one test still covers every workflow.)*
- **The toolchain index is assembled from the releases.** Each release (and nightly) attaches `index-entry.json`, its own entry written by `cargo xtask dist index`, and (since 2026-11-23) `index.json`, every published release's entry merged with its own (`scripts/ci/release-index.sh`, `cargo xtask dist merge-index`); `publish-index` rebuilds the published release's copy. So nothing is committed back by CI, and the newest release always carries the current index. Yanking, which must survive that, is planned with `cargo xtask release yank`.
- **Archives are written by `xtask`** (`cargo xtask dist archive`, over `flate2`, which the workspace already has) rather than by each runner's `tar` or `zip`, so they are the same on every runner and reproducible: entries carry a fixed time and mode.
- **Linux builds run in `rust:bullseye`** (Debian 11, glibc 2.31) on the native x86_64 and arm64 runners, which gives the glibc baseline without a cross toolchain.
- **The toolchain image is `debian:bookworm-slim`, not `distroless/cc`**, plus CA certificates and `k`: it is a build stage, and Dockerfiles (`examples/shop/Dockerfile`, the samples in [38-deployment.md](38-deployment.md)) run `RUN k check` in shell form, which needs `/bin/sh`. The runtime image of an app stays `distroless/cc` with `dist/app` alone.
- **A few steps are scripts** (`scripts/ci/`): those that need `gh` (the index, the nightly issue, publishing a draft), `git` with logic (the release tag, since 2026-11-23) or `cosign` (signing, verifying). Everything else is a `cargo` or `cargo xtask` command, and the test rejects any other program but a short list of tools (`rustup`, `git`, `docker`, `npm`/`npx` in `editors/vscode` only, `cosign`, `gh`, `tar`, `unzip`, `mkdir`).

### Docs freshness checks

The book already can't drift in two ways: its `k` blocks compile, and the diagnostics and standard library references are regenerated from the compiler (`K_BLESS=1`). The full set of drift checks (samples that run, command transcripts, tutorial projects, generated references, coverage, links, Planned notes tied to probes, the staleness list) is designed in [49-documentation.md](49-documentation.md#drift-proofing); they are ordinary `cargo test`s, so the gate runs them on every target. The release adds two of its own:

- **Version strings in the book** (install commands, image tags, `k = "…"` in samples) equal the current minor, or are written `{{k_version}}` and substituted when the site is built. The check is part of `crates/k_cli/tests/version.rs`. The snippet test (`book.rs`) substitutes `{{k_version}}` the same way before compiling, so a `k.toml` sample is checked with the value readers see.
- **The page audit is current:** every page in `SUMMARY.md` has an entry in the audit ledger (`docs/book/check/audit.toml`, [docs/compiler/documentation.md §5](../compiler/documentation.md#5-the-page-audit)), and `cargo xtask release prepare` lists the pages not re-audited since the previous minor's cut whose design topic changed, for the release manager to review before the candidate.
- **No fragment remains** in `changes/` on the tag, and the changelog page and the upgrade guide's new section exist.

## Docs versioning

How the site is organised (Learn, Guides, Reference, the quick start and the tutorial), how it is searched and how pages move without breaking links is [49-documentation.md](49-documentation.md). This section fixes what the release guarantees about it.

- **The docs site is built from the release tag,** by the release pipeline: the book with the version substituted, plus `index.json`, `versions.json` and the install scripts. So every snippet on `/0.6/` compiled against 0.6's compiler, every generated reference page is 0.6's, and the tutorial projects passed 0.6's tests.
- **Layout:**

| Path | Contents |
|---|---|
| `/0.6/…` | the book as of the newest 0.6 patch; one directory per minor (patches replace it) |
| `/next/…` | main, rebuilt nightly, with a banner saying it describes unreleased work |
| `/`, `/latest/…` | redirect to the newest stable minor's same page |
| `/versions.json` | the live minors, which one is latest, and next: read by the theme's version switcher |
| `/index.json` | the toolchain index for `k toolchain` (above) |
| `/install.sh`, `/install.ps1` | the install scripts |

- **Before 1.0 only the newest minor and `next` stay live** ([49](49-documentation.md#docs-and-releases)); an older minor's built site is attached to its release as an archive, and `/0.N/<page>` redirects to the newest minor's same page, or the page it was moved to (the redirect table), or the landing page. From 1.0 every minor stays live, and an older minor's pages carry a banner and a canonical link to the newest one, so search engines send readers to current docs.
- **A docs fix to a released minor** goes to its release branch and redeploys `/0.N/`. It needs no toolchain release, and the branch's own tests still compile its snippets with its compiler.
- **One upgrade guide, one changelog page, both cumulative.** `apps/upgrading.md` ([49](49-documentation.md#docs-and-releases)) is a hand-written procedure followed by one generated section per minor ("Upgrading to 0.6"), assembled from that minor's `breaking` and `deprecation` fragments with their migrations, newest first, between `<!-- gen:upgrading -->` markers. **The changelog page** (`reference/changelog.md`) is `CHANGELOG.md`. Both keep every earlier minor's sections, so the one live minor's site (before 1.0) still tells a project two minors behind everything it needs. The release notes link to both.
- **`k doc` output for a package** is the package's own and is not hosted by the project.

#### Docs versioning: decided while implementing (2026-11-12)

`cargo xtask docs build` ([release.md §12 step 12](../compiler/release.md#12-implementation-plan)):

- **One run builds one version into a site directory and rewrites the root files**; a full site is two runs, `--version latest` (the newest stable minor's book from its newest tag, through a temporary worktree) and `--version next`. GitHub Pages replaces the whole site on each deploy, so each deploy builds both.
- **Substitution happens on a staged copy** of `docs/book`, not in an mdBook preprocessor: a plain `mdbook build docs/book` then needs no `xtask` build, and `{{k_version}}` stays visible there, which the snippet test also expands. The highlighting preprocessor ([documentation.md §4](../compiler/documentation.md#4-highlighting-planned)) is still planned.
- **mdBook's errors and warnings fail the build**, since mdBook 0.5 exits 0 after some of them (an unknown `book.toml` key).
- **`versions.json`** is `{"format": 1, "latest": "0.6", "live": ["0.6"], "next": true}`. A minor counts as released when a stable tag (`vX.Y.Z`, no pre-release) exists, when it is being built, or when its directory is already in the site, so a local build before any tag links to itself. Before any release, `/` and `/latest/` go to `/next/`.
- **Redirects are static pages**, since GitHub Pages has no redirect rules: `index.html`, one page per page of the newest minor under `latest/`, each relative so the site works below a project path, and a `404.html` that reads `versions.json` to send `latest/…` and a retired minor's page to the newest minor's same page, and a missing page of a live version to its landing page. The moved-page redirect table joins it with `released-pages.txt` (planned).
- **The version switcher and the banners** are `docs/book/theme/k-version.js`, loaded on every page: it finds the version from the page's own path, fetches `versions.json`, and adds a menu-bar switcher that keeps the page, and a banner on `/next/` ("unreleased work on main") and on an archived minor. Outside a version directory (a plain build, `mdbook serve`) it does nothing.
- **The install scripts** are copied to the root from the newest minor's source (or from the first build when nothing is released). Until 2026-11-23 the copy had the release host and the index filled in (`@K_RELEASES@`, `@K_TOOLCHAIN_INDEX@`, from `--releases` and `--index`); since the [private distribution](#private-distribution-decided-2026-11-23) the scripts name the GitHub releases (and, since the [public distribution](#public-distribution-decided-2026-11-23), the site's `index.json`) themselves and need nothing filled in, and the options are gone.

## Owner decisions

Each needs the project owner's sign-off before anything is published. The owner decided several on 2026-11-09 and the name, namespace, docs domain and licence on 2026-11-23, then that the repository stays private, and later the same day that it is **public** (also 2026-11-23; the last column); **nothing is published yet**: what remains before the first publication is the owner's starting it (`gh workflow run release.yml -f version=…`, which tags, then attaching the macOS archive and `gh workflow run publish.yml`; [release.md §12, step 15](../compiler/release.md#12-implementation-plan)). Everything else in this file, and in [49-documentation.md](49-documentation.md), can be built and tested before they are made. **This table is the one list for both topics**; 49 refers to it.

| Decision | Options | Recommendation | Owner's decision |
|---|---|---|---|
| **Project name and namespace** (the organisation was a placeholder) | the name "K" is already used by the APL-family language from Kx Systems, and short package names (`k` on Homebrew, crates.io) may be taken; a check is needed before any publication | decide before the first public release | **Decided (2026-11-23):** the language is **Kobra**. Source files keep `.k`, the command stays `k`, and so do `k.toml`, `k.lock`, the `K_*` variables, the `K####` codes, the `k.*` standard library modules and the `k_*` crates: only prose and display names say Kobra. The namespace is the owner's GitHub account: the repository `github.com/slavkobojanic/kobra`, the image `ghcr.io/slavkobojanic/kobra`, the VS Code extension `slavkobojanic.kobra` (its `.vsix`, `kobra-<version>.vsix`, on each GitHub release). They are written once, in `Cargo.toml`'s `[workspace.metadata.project]`, which the docs build and the version bump read and `crates/k_cli/tests/version.rs` checks the rest against |
| **Source hosting and CI** | GitHub with Actions; GitLab CI; sourcehut; Buildkite with self-hosted agents | GitHub Actions: native runners for all six targets, free for public repositories, keyless signing through its OIDC tokens | **Decided (2026-11-09):** GitHub Actions. **Runners decided (2026-11-23):** no macOS runners at all (a private repository pays ten Linux minutes for each macOS minute); the change checks run on Linux only; releases build Linux and Windows, and the owner's Mac builds the macOS archive ([Private distribution](#private-distribution-decided-2026-11-23)). **Triggers decided (2026-11-23):** every workflow runs only when started by hand, and the release workflow creates its tag ([CI on request](#ci-on-request-decided-2026-11-23)). Both stay now that the repository is public ([Public distribution](#public-distribution-decided-2026-11-23)) |
| **Release binaries host** | GitHub Releases; an object store behind the docs domain (S3, R2, GCS) | GitHub Releases for the archives; `index.json` on the docs site so the host can change without a toolchain release | **Decided (2026-11-09):** GitHub Releases, with `index.json` on the docs site. **Revised (2026-11-23):** the repository stays **private**: GitHub Releases of `slavkobojanic/kobra`, downloaded with the collaborator's token, and `index.json` attached to every release ([Private distribution](#private-distribution-decided-2026-11-23)). **Revised again (2026-11-23):** the repository is **public**: the same releases, downloaded without a token, with `index.json` on the docs site too ([Public distribution](#public-distribution-decided-2026-11-23)) |
| **Signing** | Sigstore keyless with build provenance; a minisign key held by maintainers; GPG | Sigstore keyless: no long-lived key to guard. **Apple notarization** and **Windows Authenticode** need paid developer accounts; without them, the install script (which avoids browser quarantine) is the supported path on macOS, and Windows shows a SmartScreen prompt | **Decided (2026-11-09):** Sigstore keyless with build provenance; no notarization or Authenticode for now |
| **Required pull-request checks** | every "Change checks" row of the gate; only tests and lints; none (advisory) | every row, docs ownership included, so a stale page blocks a merge rather than a release | open; since 2026-11-23 the checks run only when started ([CI on request](#ci-on-request-decided-2026-11-23)), so requiring them means starting `ci.yml` on each pull request's branch |
| **Docs site host and domain** | GitHub Pages; Cloudflare Pages; Netlify; object storage behind a CDN | GitHub Pages if the source is on GitHub (no extra account, deployed by the same pipeline); the layout above is plain directories, so moving later is cheap ([49](49-documentation.md#hosting) compares them). The domain waits for the name | **Decided (2026-11-09):** GitHub Pages. **Domain decided (2026-11-23):** `https://slavkobojanic.github.io/kobra/`, the repository's Pages site, with no custom domain. **Deferred (2026-11-23):** while the repository is private there is no site (the owner's GitHub Free plan has no Pages for a private repository): each release attaches its book, `k docs` opens the installed copy, and collaborators install with `gh api repos/slavkobojanic/kobra/contents/scripts/install.sh -H 'Accept: application/vnd.github.raw' \| sh` (`gh api repos/slavkobojanic/kobra/contents/scripts/install.ps1 -H 'Accept: application/vnd.github.raw' \| Out-String \| iex` on Windows). The Pages jobs stay, behind `K_PAGES_ENABLED`, for the day it is public. **Site live (2026-11-23):** the repository is public, Pages is enabled (built by the workflows) and `K_PAGES_ENABLED` is `true`, so the site is the versioned book with `install.sh`, `install.ps1` and `index.json` at its root, and the one-liners are `curl -fsSL https://slavkobojanic.github.io/kobra/install.sh \| sh` and `irm https://slavkobojanic.github.io/kobra/install.ps1 \| iex` ([Public distribution](#public-distribution-decided-2026-11-23)) |
| **Docs pull-request previews** | none; the host's previews (Cloudflare Pages, Netlify); the built site as a CI artifact | the built site as a CI artifact at first: no second host | open |
| **Pagefind for search** | keep mdBook's search; Pagefind at docs-build time | Pagefind, pinned to one version and checksum in `xtask`; it never reaches users' machines ([49](49-documentation.md#search)) | open |
| **Container registry** | GHCR; Docker Hub; both | GHCR, alongside the source | **Decided (2026-11-09):** GHCR |
| **Extension marketplaces** | VS Code Marketplace (an Azure DevOps publisher); Open VSX (an Eclipse account) for VSCodium and others; `.vsix` on the release only | both marketplaces once the name is settled; the `.vsix` until then | **Decided (2026-11-09):** the `.vsix` attached to each release; the marketplaces wait for the name. Now that the name is settled (2026-11-23), the extension is `slavkobojanic.kobra`, ready for a publisher account of that name; publishing to a marketplace is still the owner's to switch on |
| **Package managers** | an own Homebrew tap, then homebrew-core; winget; Scoop; AUR; Nix; crates.io | an own Homebrew tap first; the rest on demand | **Decided (2026-11-09):** an own Homebrew tap first |
| **Performance runner** | a self-hosted machine; a dedicated cloud instance; a maintainer's machine, by hand | a self-hosted runner | open |
| **First public release and the train's start** | `0.1.0` now; after the quick start, the tutorial and toolchain pinning exist; after packages and `k fmt` | after rounds 1–4 of the [combined plan](49-documentation.md#implementation-plan) and a local `0.1.0-rc.1` dry run: a newcomer's first visit then has a quick start that works and a project they can pin | **Decided (2026-11-12):** `0.1.0` is the built toolchain and examples, plus every step of [release.md §12](../compiler/release.md#12-implementation-plan) (steps 1–15, the `0.1.0-rc.1` dry run included), plus `k fmt`. Packages, Protobuf, HMR, WebKit and the other Planned features come after 0.1. Nothing is published, no account is used and no tag is pushed until the owner switches publishing on |
| **Licence** | the project's licence (MIT, Apache-2.0, both, or another), found missing by the `0.1.0-rc.1` dry run | a permissive licence, chosen before anything is published | **Decided (2026-11-23):** dual **MIT OR Apache-2.0**, the Rust convention: `LICENSE-MIT` and `LICENSE-APACHE` at the root (copyright 2026 Slavko Bojanic and the Kobra contributors), `license = "MIT OR Apache-2.0"` in the workspace `Cargo.toml` (every crate inherits it) and the VS Code extension, and both files in every release archive (`cargo xtask dist archive` refuses to pack without them) |
| **Publishing the internals** (`docs/design`, `docs/compiler`) | a second small mdBook; the code host's markdown rendering; not at all | the code host's rendering; revisit after 1.0 | open |
#### The first dry run: decided (2026-11-12)

[release.md §6](../compiler/release.md#6-cutting-a-minor-release) was run end to end for `0.1.0-rc.1` in a scratch clone, with nothing pushed or published ([release.md §12, step 15](../compiler/release.md#12-implementation-plan) has the results). What it settled:

- **A pre-release is a version of its minor everywhere a minor is asked for.** `k new` on `0.1.0-rc.1` writes `k = "0.1"` (it wrote `0.1.0-rc`, which no toolchain satisfies), and `k upgrade` running on a candidate takes the project through the candidate's own minor; installed pre-releases of other minors still never count. A pre-release still runs a project locked to a release of its minor only by handing the command over: it is never the same release line.
- **`k check` reports an app's own `k.toml` problems** (`K8101`) as it did a workspace member's; an app's malformed `k` passed silently.
- **The version moves in the test projects too.** `release prepare` and `start-next` bump the `k` and `k.lock` of `tests/e2e/` and each crate's `tests/` (not `tests/corpus`, `tests/compat` or `xtask`'s fixtures), and the manifests the tests write take the minor from the crate version; `version.rs` checks both.
- **A fresh checkout runs the gate.** Test fixtures are never git-ignored (a test project's `.env` is committed; `crates/xtask/tests/checkout.rs`), and the fuzz jobs seed `fuzz/corpus` first (`fuzz/seed-corpus.sh`, which seeds every target).
- **Release notes link the repository at the tag**: `cargo xtask ci notes --links <repository>/blob/<tag>` makes `CHANGELOG.md`'s relative links absolute, since a release page doesn't resolve them.
- **With only candidates published**, `k toolchain install` and `k self update` say there is no stable release and name the candidate or `--version`; a candidate is installed only when named exactly. (The install scripts first did the same; since 2026-11-23 they install the newest candidate, below.)

#### The name, the namespace and the licence: decided (2026-11-23)

- **Kobra in prose, `k` everywhere else.** The book's title, the README, the editor extensions' display names, `k --help` (the `k_cli` description), the compiler's messages that name the language ("Kobra has no ternary operator") and the release titles say Kobra. The command, the `.k` extension, `k.toml`, `k.lock`, `K_*`, `K####`, `k.*` and the `k_*` crates are unchanged, as are the LSP's `serverInfo.name` (`k`, the server's command) and `k --version` (`k 0.1.0`, which scripts parse). The example apps' product names (K Shop, K Notes, the K Mug) are left as they are: they are names inside the apps and their tests, not the language's.
- **One table holds the namespace.** `Cargo.toml`'s `[workspace.metadata.project]` has `name`, `repository`, `site`, `image` and `extension`. `cargo xtask docs build` fills the install scripts in from it (`--releases` and `--index` override), `release prepare` finds the image tags to move by its `image`, and `version.rs` fails when a workflow, a Dockerfile, the extension's `package.json`, the README or the book names another place, when a placeholder namespace is left, or when a licence file is missing. The workflows name the image and the index once each (`K_IMAGE`, `K_INDEX_URL`), not through `github.repository`, so a fork can't publish under another name by accident.
- **The install scripts install the newest pre-release while there is no stable release.** GitHub's `latest` alias skips pre-releases, so with only `0.1.0-rc.1` published, `curl … | sh` would otherwise fail on its first public day. When `latest/download/SHA256SUMS` is missing and no version was asked for, the scripts read the site's `index.json` (`K_TOOLCHAIN_INDEX`, filled in by the docs build) and take the newest release that isn't yanked and has an archive for the machine, a stable release before a candidate before a nightly; they print "there is no stable release of Kobra yet; installing the newest release candidate, 0.1.0-rc.1" and still download from GitHub Releases and check `SHA256SUMS`. Without an index they fail as before, naming `--version`. `install.sh` parses the index with `awk` (no `jq`); `install.ps1` with `ConvertFrom-Json`. `k toolchain install` keeps "never a candidate unless named": a user of `k` asked for something, and a project's pin says which.
- **Release builds get the built-in index** from the workflows (`K_DEFAULT_TOOLCHAIN_INDEX` on the build step only, so the test jobs' `k` still has none). *(Superseded the same day by [Private distribution](#private-distribution-decided-2026-11-23), which also replaced the site's `index.json` and the `curl … | sh` one-liners below.)*
- **The toolchain image is labelled** with its source repository and licence (`org.opencontainers.image.source`, `…licenses`), which also links the GHCR package to the repository.

#### Private distribution: decided (2026-11-23)

*(Its consequences for installing, the token and the docs site were superseded the same day by [Public distribution](#public-distribution-decided-2026-11-23); the book attached to each release, `k docs`, the index attached to each release and the Linux-and-Windows-only workflows with `attach-local` for macOS stay. Kept as written, for the history.)*

The owner decided that `github.com/slavkobojanic/kobra` **stays private**: Kobra only needs to install for people who have access to the repository. The owner's GitHub plan is Free, which has no GitHub Pages for a private repository, and a private repository's release assets are downloaded only with a token. What follows from it:

- **Install with the GitHub CLI.** A collaborator signs in with `gh auth login` and runs `gh api repos/slavkobojanic/kobra/contents/scripts/install.sh -H 'Accept: application/vnd.github.raw' | sh` (on Windows, `gh api repos/slavkobojanic/kobra/contents/scripts/install.ps1 -H 'Accept: application/vnd.github.raw' | Out-String | iex`; `… | sh -s -- --version 0.1.0` for a version). `gh api` fetches the script from the repository; the script downloads `SHA256SUMS`, the archive and the book with `gh release download -R slavkobojanic/kobra`, or, without `gh` but with `GH_TOKEN` (or `GITHUB_TOKEN`), with curl through the REST API (`releases/tags/<tag>`, then the asset's API URL with `Accept: application/octet-stream`). It checks the repository first, so a 404 says "you may not have access to slavkobojanic/kobra" with what to do (`gh auth login`, or a token that can read it) instead of looking like a missing release. With no stable release (`releases/latest` is 404), it reads the `index.json` of the newest published release and installs the newest candidate, as before. JSON is read with a small `awk` flattener in `install.sh` (no `jq`) and `ConvertFrom-Json` in `install.ps1`.
- **`k toolchain install` and `k self update` authenticate** (`crates/k_cli/src/github.rs`): the token is `GH_TOKEN`, else `GITHUB_TOKEN`, else what `gh auth token` prints, asked only when needed. It is sent **only to the API's origin** (`https://api.github.com`, or `K_GITHUB_API` in the tests): never to an index or archive on another host (`K_TOOLCHAIN_INDEX` still overrides, which is how the loopback tests run without a token), and never to the storage host an asset's API URL redirects to, whose URL is signed already; `k_server::outbound::download_with` asks for each hop's headers, so a redirect can't carry it. Without `K_TOOLCHAIN_INDEX` the index is the `index.json` asset of the newest published release (largest `published_at`, drafts skipped; a draft published late is still the newest), and an archive URL `https://github.com/<repo>/releases/download/<tag>/<file>` is fetched as that release's asset. No token: the command says to run `gh auth login` or set `GH_TOKEN`, and downloads nothing.
- **The index travels with the releases.** Every release and nightly attaches the whole `index.json` (every published release's `index-entry.json` merged with its own), and `publish-index` refreshes the published release's copy, so the newest release always lists every release. The index's archive URLs stay the public form (`https://github.com/…/releases/download/…`), which a public repository serves as they are.
- **The docs ship as a release asset.** Each release attaches `docs-<version>.tar.gz` (`cargo xtask dist docs`, the tag's built book below `docs-<version>/`), listed in `SHA256SUMS` and as the index entry's optional `docs` (`url`, `size`, `sha256`; still format 1). The install scripts and `k toolchain install` unpack it into `~/.k/docs/<version>/` (a failure there is a warning, never a failed install), and **`k docs`** opens the running toolchain's copy with `$BROWSER` or the system's opener, or prints its path with `--path`; without it, it says so and names `mdbook serve docs/book` for a checkout. It doesn't delegate: it is this `k`'s book.
- **No macOS runners.** A private repository's macOS minutes are billed at ten times Linux's (Windows at twice), so no workflow runs on macOS: the change checks run on Linux only, and a release builds and smoke-tests the Linux and Windows targets. The release manager builds `aarch64-apple-darwin` on the owner's Mac with `cargo xtask release attach-local <tag>` ([release.md §6](../compiler/release.md#6-cutting-a-minor-release)): it builds with `--locked`, smoke-tests, archives, and attaches the archive to the existing draft with `gh release upload`, merging its line into `SHA256SUMS` and its entry into `index-entry.json` and `index.json` (an archive of the same target is replaced, so a rerun is safe). `x86_64-apple-darwin` isn't offered: building it on an arm64 Mac needs V8's x86_64 library, an extra download, and Rosetta to smoke-test it. The local archive has no Sigstore bundle or provenance (those need CI's identity), only its `SHA256SUMS` digest. Nightlies have no macOS archive.
- **What changes if the repository goes public:** set `K_PAGES_ENABLED` (and Pages' source) and the Pages jobs deploy the versioned site, `install.sh` and `install.ps1` at its root and `index.json`, as designed above; the install one-liners can become `curl -fsSL https://slavkobojanic.github.io/kobra/install.sh | sh` and `irm …/install.ps1 | iex`, the scripts can download without a token (curl from `…/releases/download/…`), and `k toolchain` can read the API without one (the index's URLs already work unauthenticated). macOS runners become free, so the macOS targets (both of them) can move back into the workflows and `attach-local` becomes a fallback. `k docs` stays useful offline.

#### CI on request: decided (2026-11-23)

The owner decided that **CI runs only when someone starts it**, and that **the release workflow creates the release's tag**. The repository is private on GitHub Free, where every Actions minute counts against the plan (Windows at twice Linux's rate), and the owner wants explicit control over what runs and when, rather than a run per push, per pull request and per day.

- **Every workflow's one trigger is `workflow_dispatch`**, started from the Actions page or with `gh workflow run <file>` (`--ref <branch>` picks the branch, whose head is checked out and whose copy of the workflow runs). No pull request, push, tag, published release or schedule starts anything; `crates/xtask/tests/workflows.rs` and the checker in `xtask/src/workflows.rs` fail on any other trigger.
- **Starting is the switch,** so `K_CI_ENABLED` is gone (the checker rejects it). `K_PAGES_ENABLED` stays for the day the repository is public. `K_PUBLISH_ENABLED` stays for the one thing it still guards that a run doesn't already decide: pushing the toolchain images to GHCR, whose private packages count against the plan's storage. Releases and nightlies publish because someone started them (`release.yml`'s `draft`, `nightly.yml`'s `publish` input). `K_PERF_RUNNER` is unchanged.
- **The nightly keeps its name.** It is now a build of main someone asks for (`gh workflow run nightly.yml`, `-f publish=false` for the checks alone) rather than a scheduled one, but the version (`X.Y.Z-nightly.<date>`), the channel and `k toolchain install nightly` already say "nightly", and renaming them to "snapshot" would change the index and the toolchain for no gain. Its date-based version allows one published nightly a day.
- **`ci.yml` compares with an input** (`base`, default `origin/main`) instead of a pull request's base: the fragments, the docs gate and which paths changed. Its Pages deploy runs for a run of main.
- **`release.yml` takes the version as an input**: `gh workflow run release.yml [--ref release/0.6] -f version=0.6.0` (`draft`, default `true`). Its first job, `check`, runs `cargo xtask ci check-version`: a version, not a nightly, equal to the branch's `Cargo.toml`, with its `CHANGELOG.md` section, and not tagged yet; each refusal names the command that fixes it. **The version must be prepared on the branch first** (`cargo xtask release prepare`, committed and pushed): the workflow doesn't run `prepare` and commit, because what is released should be a commit a person read, and CI then writes nothing to a branch. Every job reads the version from `check`, never from a ref's name, and checks out the dispatched commit. There is no `ref` input: `--ref` already picks the commit, and keeping it the dispatch ref means the workflow file and the code it gates are the same commit.
- **The tag is made last, by one job.** `release`, the only `release.yml` job with `contents: write`, runs only when the gate, the builds, the smoke tests, the signature check, the extension, the images and the book passed (`!cancelled() && !failure()`, so a skipped `gate-perf` doesn't block it). It creates the annotated tag `v<version>` on the dispatched commit and pushes it (`scripts/ci/tag.sh`: a re-run finds it on that commit and goes on; a tag on another commit is refused, since tags are never moved), then the release with `gh release create --verify-tag`: a draft by default, a pre-release for a version with a pre-release part, carrying everything `attach-local` merges into (`SHA256SUMS`, `index-entry.json`, `index.json`). A red gate leaves no tag behind, so the version can be fixed and started again.
- **Publishing is `publish.yml`** (`gh workflow run publish.yml -f version=0.6.0`), since there is no `release: published` event to run on: it checks the tag exists (`ci check-version --tagged`), refuses a release without the macOS archive unless `-f macos=false`, publishes the draft, refreshes its `index.json`, and, behind their switches, tags the multi-arch image and deploys Pages. So the release procedure is `release prepare` → commit and push → `release.yml` → `gh run watch` → `attach-local` → `publish.yml` ([release.md §6](../compiler/release.md#6-cutting-a-minor-release)).
- **`attach-local` is unchanged**: it merged into an existing release, which the workflow still makes; its messages now name the workflow and `git fetch origin tag <tag>`, since the tag is made on GitHub.
- **A candidate keeps its fragments**, so the release gate runs `cargo xtask fragments --none-left` only for a release and `cargo xtask fragments` for a candidate (the old tag-triggered gate would have failed every candidate there).

#### Public distribution: decided (2026-11-23)

The owner made `github.com/slavkobojanic/kobra` **public** the same day, enabled GitHub Pages with the workflows as its source (`build_type=workflow`, at `https://slavkobojanic.github.io/kobra/`) and set `K_PAGES_ENABLED=true`. CI stays as [CI on request](#ci-on-request-decided-2026-11-23) has it (`workflow_dispatch` only), and with no macOS runners: macOS minutes would now be free, but the owner kept the workflows as they are, so `attach-local` stays the way the macOS archive is made. This supersedes [Private distribution](#private-distribution-decided-2026-11-23) where it said what a private repository forced, and keeps what still helps:

- **The install one-liners are the site's again:** `curl -fsSL https://slavkobojanic.github.io/kobra/install.sh | sh` and `irm https://slavkobojanic.github.io/kobra/install.ps1 | iex` (`… | sh -s -- --version 0.1.0` for a version), the URLs of the earlier public design. `cargo xtask docs build` copies `scripts/install.sh` and `install.ps1` to the site's root unchanged (they name the repository and the site themselves, so nothing is filled in), and the Pages jobs (`ci.yml`'s for a run of main, `publish.yml`'s for each published release, both behind `K_PAGES_ENABLED`) deploy them with the versioned book and `index.json` (`scripts/ci/release-index.sh`). `version.rs` checks the README, the installation page, the quick start and the JetBrains plugin's notification name them, and that no `gh api …` one-liner is left.
- **The scripts need no sign-in.** They download `SHA256SUMS`, the archive and the book with plain curl (or wget; `Invoke-WebRequest` in `install.ps1`) from `https://github.com/slavkobojanic/kobra/releases`: `latest/download/SHA256SUMS` for the newest stable, then that release's `download/v<version>/…` (so a release published meanwhile can't mix two versions), or `download/v<version>/…` for `--version`. No request carries a token. Checksum verification is unchanged. The repository-access check and its "you may not have access" messages are gone.
- **No stable release yet → the newest candidate, as before:** with `latest/download/SHA256SUMS` missing (GitHub's `latest` skips pre-releases), the scripts read the site's `index.json` and take the newest release that isn't yanked and has an archive for the machine, saying so. If the site has no `index.json` (Pages not deployed yet), they fall back to the API: the newest published release with an `index.json` asset (`repos/<repo>/releases?per_page=100`), whose `index.json` they download from `download/<tag>/index.json`. That is the only API request the scripts make, and the only one that carries a token: `GH_TOKEN`, else `GITHUB_TOKEN`, else `gh auth token` when `gh` is installed (`K_INSTALL_GH=none` turns that off), so a user behind GitHub's limit of 60 unauthenticated requests an hour can lift it. `gh` and a token are never required. `K_RELEASES`, `K_TOOLCHAIN_INDEX` and `K_GITHUB_API` point the scripts at loopback in the tests.
- **`k toolchain install` and `k self update` download without a token.** They still read the index from the newest published release's `index.json` asset and each archive as its release's asset, through GitHub's API, which serves a public repository's assets unauthenticated; with no token they now download instead of refusing. The token, when one is found (`GH_TOKEN`, `GITHUB_TOKEN`, `gh auth token`), is still sent only to the API's origin and never to the storage host a redirect leads to, which keeps the tests of that. `k` stays on the API rather than the site's `index.json` and the `github.com` download URLs so that its tests stay on loopback through `K_GITHUB_API` and one code path serves both cases; an install makes about six API requests. The API's refusals say what they mean: 404 that the repository has no such release or file, 403 and 429 its rate limit and how to lift it (a token, or `gh auth login`), 401 a token it refused.
- **The docs site is the docs again**, as [49](49-documentation.md#hosting) designs it; **`k docs` stays**, since an installed toolchain's own book reads offline and matches its version. Without the book it now also names the site's copy: `https://slavkobojanic.github.io/kobra/<minor>/`, or `…/next/` for a nightly. Releases keep attaching `docs-<version>.tar.gz` and `index.json`.

## Why

- **One version for every part** follows from one binary: the compiler, the stdlib and the runtime are already inseparable, and an extension with its own version would only add a compatibility matrix.
- **Explicit pins with delegation** give reproducible builds (the `rust-toolchain.toml` lesson) without making a fresh checkout reach the network (the reason not to copy Go's automatic download).
- **A fix for every break** follows from the machine-applicable fixes the compiler already has: Kobra can promise that upgrading is a command, and agents can apply the same fixes from JSON.
- **Adjacent-minor wire compatibility** is the toolchain's half of what `rpc.lock` does for protocols: services and open tabs outlive a deploy.
- **Fragments over conventional commits** keep each changelog entry next to the code it describes, reviewed with it, and carrying its migration.
- **Docs from the release commit** make the book's existing guarantees (snippets compile, references are generated) hold per version instead of only for main.

## Open questions

- Whether a public standard library enum can be marked as one that will gain cases (Rust's `#[non_exhaustive]`), so adding a case to `RpcFailure` or `HttpError` isn't breaking, or whether every such addition waits for a minor. A caller can already keep a complete catch list working across such an addition with `catch unknown` ([09](09-errors.md#cases-added-later-catch-unknown)): the build warns (K2207) instead of failing; a `match` over one still breaks.
- Whether user declarations shadow names in the implicit imports; if they do, adding a name there isn't breaking.
- Whether `dist/app` should be able to come from a slimmer runtime binary without the compiler ([architecture.md §14](../compiler/architecture.md#14-projects-commands-and-build-outputs)), which would add a second artifact per target.
- How long a release candidate soaks (one week is assumed) once there are users to test it.

## Windows: deferred (2026-11-23)

The first release run was also the first time the suite ran on Windows, and about 470 tests failed there (line endings, paths, Unix-only test helpers). Windows is taken out of the release and nightly builds until the suite passes on it; 0.1 ships for Linux (x86_64, arm64) and macOS (Apple Silicon, attached locally). `install.ps1` stays, for when a Windows archive exists.
