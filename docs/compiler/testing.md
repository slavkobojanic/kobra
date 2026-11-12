# Testing

K is built test-first: tests for a stage are written before its implementation, and they must cover edge
cases exhaustively. This document explains every kind of test, how to run it, and exactly how to add
cases. Most of it can be followed without any implementation in place.

Contents:

1. [Test kinds at a glance](#1-test-kinds-at-a-glance)
2. [Running tests](#2-running-tests)
3. [Golden corpus](#3-golden-corpus)
4. [Unit and integration tests](#4-unit-and-integration-tests)
5. [Property tests](#5-property-tests)
6. [Fuzzing](#6-fuzzing)
7. [Regression tests](#7-regression-tests)
8. [End-to-end tests](#8-end-to-end-tests)
9. [Benchmarks and performance budgets](#9-benchmarks-and-performance-budgets)
10. [Planned suites](#10-planned-suites)

---

## 1. Test kinds at a glance

| Kind | Where | What it checks |
|---|---|---|
| Unit | `#[cfg(test)] mod tests` inside each source file | One function or type in isolation |
| Integration | `crates/<crate>/tests/*.rs`, modules of the crate's one test binary `it` (`tests/main.rs`, §2.4) | A crate's public API |
| Golden corpus | `tests/corpus/<suite>/**/*.k` + expectation files | Exact output of a compiler stage for a K source |
| Property | `crates/<crate>/tests/properties.rs`, `invariants.rs` | Invariants over generated inputs (proptest) |
| Differential | `crates/k_server/tests/differential.rs`, `crates/k_db/tests/sql_properties.rs`, `crates/k_codegen/tests/persistence_properties.rs`, `crates/k_server/tests/literals.rs` | Generated programs compiled and run in V8 agree with a reference, across builds and between server and client rendering; SQL never holds a value (§5.6); the compiler's checked-literal parsers (`k_sema::literals`) give exactly what the runtime's `parseEmail`, `parseUrl` and `parseUuid` give, on fixed and generated inputs |
| Fuzz | `fuzz/fuzz_targets/*.rs` | No panics, no invariant violations on arbitrary and generated inputs (libFuzzer) |
| Regression | `tests/corpus/<suite>/regressions/` | A fixed bug stays fixed |
| Codegen golden | `tests/corpus/codegen/**` | Generated client JS, server JS and CSS, or assertions about them (§3.9) |
| Execution | `tests/corpus/run/**` | A program's log output and result after compiling and running it in V8: the main behavioural suite for language semantics (§3.10) |
| Runtime JS | `crates/k_runtime/js/tests/*.test.js` | The JavaScript runtime, inside V8 with a fake DOM (§4.1) |
| Database | `crates/k_db/tests/*.rs` | Drivers, SQL, the migration builder and migrations on SQLite, and on Postgres when configured (§4.2) |
| Database corpus | `tests/corpus/db/<app>/` | Apps with entities, repositories and migrations served on a fresh database and driven by `requests.http` (§3.11) |
| Release builds | `crates/k_codegen/tests/release.rs`, `crates/k_codegen/src/{minify,link,shorten}.rs` (unit) | Every example's release client: named runtime imports, a runtime chunk with only what the app reaches, a source map per chunk, no chunk naming a file that isn't shipped, `walk` paths, quoted wire keys, short CSS names that agree across CSS, markup and SSR, `modulepreload` lists, and each app's gzip budget from performance.md §11.7 ([codegen.md §17.1–17.6](codegen.md#171-release-linking-named-runtime-imports-and-a-shaken-runtime)) |
| Examples | `crates/k_cli/tests/examples.rs` | Each example builds with `k build`, its `dist/app` serves its main page and an rpc (§8.1) |
| Browser | `crates/k_server/tests/browser.rs` | Each example's main pages, server-rendered, hydrated by the generated client in the fake DOM and driven through DOM events (§4.3) |
| Real browser | `crates/k_cli/tests/chrome.rs`, `crates/k_cli/tests/firefox.rs`, `crates/k_cli/tests/engines.rs`, `crates/k_cli/tests/example_checks.rs` | Each example as its README runs it, in headless Chrome over the DevTools protocol: real input, layout, styles, fonts and focus; the stdlib's crypto, web-platform and media bindings on both Chromium and Firefox (over WebDriver BiDi); and the examples' setup scripts and configuration files (§4.4) |
| End-to-end | `crates/k_cli/tests/`, `tests/e2e/`, `crates/k_server/tests/http/` | The `k` binary, built apps, HTTP (§8) |
| Compose smoke | `crates/k_cli/tests/compose.rs` | Each example's Docker Compose stack, built and started for real, used over HTTP and torn down; only with `K_COMPOSE=1` (§8.6) |
| Kind smoke | `crates/k_cli/tests/kubernetes.rs` | The shop's Kubernetes manifests applied to a throwaway kind cluster, used over HTTP through a Service, rolled with `kubectl rollout restart` and deleted; only with `K_KIND=1` (§8.7) |
| Language server | `crates/k_lsp/src/**` (unit), `crates/k_lsp/tests/{server,properties,editors}.rs`, `crates/k_cli/tests/lsp.rs` | The features on fixtures, the server in process, random edits and requests, the editor grammars, and `k lsp` driven over JSON-RPC against the examples (§8.5) |
| Compatibility | `crates/k_server/tests/compat/` over `tests/compat/` | Every lockfile format's golden file is read and written back unchanged, a newer format is `K8202`; each minor's recorded wire transcripts are answered as recorded by the current server and decoded by the current client (§8.8) |
| Benchmark | `crates/<crate>/benches/*.rs` (criterion), `tests/perf_budget.rs` | Throughput and latency against the budgets |

---

## 2. Running tests

Every command assumes `. "$HOME/.cargo/env"` has been run in the shell.

**Day to day, run `scripts/affected-tests.sh`** (only what your change can break) **or `scripts/test.sh`**
(everything, in about half a minute); see [§2.1](#21-running-tests-fast). Agents run affected tests only
([§2.2](#22-affected-tests)). The `cargo test` commands below still work and are the way to pass a suite its
own arguments (`K_BLESS=1`, `-- --exact`, `PROPTEST_CASES=…`).

```sh
cargo test --workspace                          # everything that runs on stable
cargo test -p k_syntax                          # one crate
cargo test -p k_corpus --test corpus            # the whole golden corpus
cargo test -p k_corpus --test corpus -- lexer/  # corpus cases whose name contains "lexer/"
cargo test -p k_corpus --test corpus -- --exact lexer/strings/escapes.k
K_BLESS=1 cargo test -p k_corpus --test corpus  # rewrite expectations from actual output
cargo test -p k_syntax --test it -- properties::        # property tests (shared oracle, see §5)
cargo test -p k_syntax --test it -- invariants::        # the original minimal invariants
cargo test -p k_sema --test it -- properties::          # semantic property tests (soracle, typed generator; §5.5)
PROPTEST_CASES=3000 cargo test --release -p k_sema --test it -- properties::   # longer semantic run
PROPTEST_CASES=20000 cargo test -p k_syntax --test it -- properties::   # longer property run
cargo test -p k_server --test it -- differential::      # stage 3 differential properties (§5.6)
PROPTEST_CASES=500 cargo test -p k_server --test it -- differential:: --test-threads=4   # longer run
cargo test -p k_db --test it -- sql_properties::        # SQL never interpolates, SQLite equals the reference
cargo test -p k_codegen --test it -- persistence_properties::   # repository SQL never holds a constant
K_STRESS_CASES=100000 cargo test -p k_syntax --test it -- robustness::  # longer random stress run
cargo test -p k_corpus --test corpus -- codegen/   # the codegen golden suite (§3.9)
cargo test -p k_server --test corpus_run        # the execution corpus, in V8 (§3.10)
cargo test -p k_server --test runtime_js        # the runtime's JS unit tests, in V8 (§4.1)
cargo test -p k_server --test it -- client_bundle::     # generated client bundles running in V8
cargo test -p k_server --test it -- cancellation::      # generated pages stop their async work when left
cargo test -p k_db                              # database tests on SQLite (§4.2)
K_TEST_POSTGRES_URL=postgres://u:pw@127.0.0.1:5432/k_test cargo test -p k_db   # and on Postgres
cargo test -p k_server --test e2e               # server end-to-end request scripts (§8.2)
cargo test -p k_server --test e2e -- db/        # the database corpus (§3.11); with K_TEST_POSTGRES_URL also on Postgres
cargo test -p k_cli --test it -- examples::             # the example apps built and served (§8.1)
cargo test -p k_server --test it -- regressions::        # adversarial findings in generated apps, pinned (§7)
cargo test --release -p k_db --test it -- perf_budget:: --ignored --nocapture --test-threads 1
cargo test --release -p k_server --test it -- perf_budget:: --ignored --nocapture --test-threads 1
cargo +nightly fuzz run parse                   # fuzzing (see §6)
cargo bench -p k_syntax                         # benchmarks (see §9)
cargo clippy --workspace --all-targets          # must be clean
cargo fmt --all --check                         # must be clean
```

Inspect what a stage produces for a file, in exactly the golden formats:

```sh
cargo run -q -p k_cli -- debug tokens path/to/file.k   # .tokens format
cargo run -q -p k_cli -- debug ast    path/to/file.k   # .ast format (canonical layout)
cargo run -q -p k_cli -- debug diag   path/to/file.k   # .diag format
cargo run -q -p k_cli -- debug sema   path/to/case     # .diag of a sema case (a .k file or a project directory)
cargo run -q -p k_cli -- check        path/to/project  # every diagnostic, human-readable (or --format json)
```

### 2.1 Running tests fast

```sh
cargo install cargo-nextest --version 0.9.111 --locked   # once; the version .config/nextest.toml expects
scripts/test.sh                  # everything: nextest, the in-process suites and the doc tests, in parallel
scripts/test.sh -p k_sema        # one package (its in-process suites and doc tests too)
scripts/test.sh -E 'test(warm)'  # a nextest filterset on top
QUICK=1 scripts/test.sh          # PROPTEST_CASES=16, K_STRESS_CASES=500 (not allowed with --ci)
scripts/test.sh --ci             # nextest's `ci` profile: longer limits, JUnit in target/nextest/ci/junit.xml
cargo nextest run -p k_server --test it differential::   # nextest directly (see the default filter below)
```

`scripts/test.sh` builds every test binary once, then runs three lanes at the same time and prints how long
each took:

| Lane | What | How |
|---|---|---|
| nextest | every test except the three suites below | [cargo-nextest](https://nexte.st): each test a process of its own, every binary at once, on every core |
| in-process | `k_corpus::corpus`, `k_server::runtime_js`, `k_server::corpus_run` | each binary run directly, all its cases in one process |
| doc | `cargo test --doc` | started once nextest is past its build, so the two cargo commands don't wait on each other's lock |

The configuration is `.config/nextest.toml`:

- **Default filter.** The three in-process suites have thousands of tiny cases; a process per case costs 0.05 s
  (corpus) to 0.4 s (V8 start-up for `runtime_js`) each, about 900 CPU-seconds in all, against 6 s when each
  binary runs its cases in one process. nextest leaves them out by default; `cargo nextest run
  --ignore-default-filter` runs them too, as processes.
- **Test groups.** `chrome` (the real-browser tests, at most 4 at once: each headless Chrome is several
  processes, and more of them only slow each other down; measured again in §4.6, where 8 and 12 made the
  whole run 2–4 s slower, not faster) and `processes` (k_cli tests that start `k`, a server or a build, at
  most 8 at once).
- **Priorities.** The dozen slowest tests and the Chrome tests start first, so no 20-second test starts last.
  Add a test to that list when it becomes one of the slowest (`final-status-level = "slow"` reports them).
- **Timeouts.** A test is reported slow after 30 s and killed after 2 minutes (`ci`: 60 s and 5 minutes), so a
  hang fails the run instead of blocking it. No retries: a flaky test fails and gets fixed.
- **Ports.** Every test process is separate, so tests that probe for a free port must reserve it across
  processes: `crates/k_cli/tests/support/ports.rs` holds an advisory file lock per port in the temp dir until
  the process exits. A program outside the tests (a `k dev` started by hand) can still take a reserved port
  before `k` binds it, so `Proc::serve` waits for `k`'s own `serving on …` line rather than for the port to
  answer, and starts `k` again on other ports when it says `cannot listen on port` (§4.6). Chrome picks its
  DevTools port itself (`--remote-debugging-port=0`, read back from `DevToolsActivePort`).
  A reservation keeps only other test processes away, so a port is handed out only when nothing else holds
  it on any address either (`ports::in_use`): nothing answers on `127.0.0.1` or `[::1]`, and `127.0.0.1`,
  `0.0.0.0` and `[::]` (where the machine has IPv6) all bind. Binding loopback alone isn't enough: on macOS
  `127.0.0.1:<port>` binds next to a server listening on `*:<port>`, so a stale `dist/app` from an earlier
  run looked free, the new app couldn't bind `0.0.0.0`, and the test talked to the stale one. Every port
  picker goes through `ports::take` (`cdp::listeners`, `cdp::free_port`, `free_ports` and Compose's
  `free_port`), and all of them stay in 5200–5999; none binds port 0. A test's own `dist/app` counts as
  started once it says `<name> serving on port <port>`, not when the port answers (`examples.rs`'
  `App::serve_on`). Tests: `ports::tests::*` (a listener on `0.0.0.0`, on loopback and on `[::]`).
- **Child processes.** Every process a k_cli test starts (`dist/app`, `k run`/`k dev`/`k lsp`, Chrome, `psql`
  under Compose) is spawned with `children::SpawnGuarded::spawn_guarded` and owned by the `ChildGuard` it
  returns, never held as a bare `std::process::Child`, which does nothing when dropped: a test that panicked
  with a server running used to leave it listening. Dropping the guard (on a panic too) lists the child's
  descendants with `pgrep -P`, `SIGKILL`s the child and then them, and reaps the child; an owner that must
  clean up after it in its own `Drop` (Chrome's profile) calls `kill_tree` first. A graceful stop
  (`SIGTERM` and `wait`, as `App::stop` does) is still the test's job; the guard is the fallback. Children
  stay in the test's process group rather than a group of their own, so a Ctrl-C at the terminal and
  nextest's signals to a timed-out test's group still reach them when the test process is killed outright
  and no destructor runs. Tests: `children::tests::*` (a panic inside `catch_unwind` with a shell and its
  `sleep` running leaves neither alive).

The build profile matters as much as the runner (`Cargo.toml`): third-party crates are built with
`opt-level = 2` and the workspace's crates with `opt-level = 1`, both with full debug info. Unoptimized,
argon2 alone made `k test examples/shop` take 14.5 s instead of 2.8 s, and the suite ran about 1.8x longer
(45 s against 25 s of nextest). Third-party crates only rebuild when `Cargo.lock` changes; after an edit to a
crate at the bottom of the graph (`k_diagnostics`), relinking every test binary takes about 10 s either way.

Measured on the 14-core development Mac (other agents building at the same time, so ±30%):

| | Before | After |
|---|---|---|
| Build every test binary, clean target | 36 s | 70 s (dependencies optimized, once) |
| Rebuild after touching `k_diagnostics` | 10 s | 7–10 s |
| Run every test | 285 s (`cargo test --workspace`, serial binaries) | 25–35 s (`scripts/test.sh`) |
| Observed in the shared `target/` with several agents | 37 min | see [§2.3](#23-several-agents-at-once) |

What was tried and not kept: `lld` (`rust-lld` as `ld64.lld`) fails to link the V8 binaries, and Apple's
default linker (ld-prime) already links one in under a second; `debug = "line-tables-only"` changed no build
time measurably.

**macOS: the first run of a new binary is checked.** macOS assesses every newly linked executable the first
time it runs (syspolicyd, about 0.7 s each, one at a time): 60–130 s for the first run after every test
binary was relinked (the in-process corpus, which runs in 0.6 s, once waited 127 s to start), and about as
much in a serial `cargo test`. Adding the terminal (or the app that runs the agents)
under System Settings → Privacy & Security → Developer Tools exempts the processes it starts. `scripts/test.sh`
can't avoid it; fewer test binaries make it shorter, which is one reason each crate has one (§2.4).

### 2.2 Affected tests

```sh
scripts/affected-tests.sh            # changes since HEAD: staged, unstaged and untracked files
scripts/affected-tests.sh main       # changes since this branch left main
scripts/affected-tests.sh --dry-run  # print the choice only
scripts/affected-tests.sh --files "crates/k_sema/src/lib.rs examples/notes/k.toml"
QUICK=1 scripts/affected-tests.sh    # fewer property-test cases while iterating
```

It lists the changed files, maps each to tests, prints every choice with its reason, and runs them through
`scripts/test.sh`:

| Changed | Runs |
|---|---|
| `Cargo.toml`, `Cargo.lock`, `.cargo/`, `.config/`, `rust-toolchain*`, `clippy.toml`, `rustfmt.toml` | everything |
| `crates/<c>/src/**`, `Cargo.toml`, `build.rs`, `js/**` | every test of `<c>` and of every crate depending on it (`cargo metadata`; a dev-dependency adds the dependent's tests but goes no further), and their doc tests |
| `crates/k_runtime/js/tests/**` | `k_server::runtime_js` and `k_runtime`'s tests (the JS test files are read from disk) |
| `crates/<c>/tests/<name>.rs`, a module of `<c>::it` | that module's tests: `binary_id(=<c>::it) & test(/^<name>::/)`; for `tests/main.rs`, all of `<c>::it`; for a binary of its own (`tests/corpus.rs`), that binary |
| other `.rs` files under `crates/<c>/tests/` (support modules) | the test modules, in any crate, that use them: through `#[path]`, or `crate::<module>` / `super::<module>` for the support modules `tests/main.rs` declares |
| other files under `crates/<c>/tests/` (fixtures, golden files) | every test binary of `<c>` |
| `examples/**`, `docs/book/**`, `docs/**`, `editors/**`, `tests/<dir>/**` | the test modules and unit tests whose code has a string literal naming that directory (`"examples"`, `"book"`, `"docs"`, `"editors"`, `"corpus"`, `"e2e"`; `corpus_root()` counts for `tests/corpus`) |
| anything else | nothing, and it says so |

The literal scan keeps the choice current as tests are added: a new test that reads `examples/` is picked up
by naming the directory as code does today. A test that finds its files some other way needs a rule in the
script's `HELPERS`.

**The rule for agents:** run `scripts/affected-tests.sh` (with `QUICK=1` while iterating, without before
reporting), not `cargo test --workspace`. Run one test file with `cargo nextest run -p <crate> --test it <name>::`
while working on it. The full `scripts/test.sh` is for changes to the workspace configuration, which the
script turns into a full run anyway, and for CI.

### 2.3 Several agents at once

Cargo lets one command at a time build in a target directory; every other `cargo` in the same checkout waits
on its lock ("Blocking waiting for file lock on build directory"). That and macOS's first-run checks after
every relink (§2.1), not the tests, are where most of the 37 minutes went: in a target directory of its own
the same `cargo test --workspace` took under 6 minutes before any of the changes above, and a first attempt to
measure in the shared one waited more than 10 minutes for the lock. Give each concurrent agent its own target
directory:

```sh
CARGO_TARGET_DIR=target/agent-<name> scripts/affected-tests.sh
```

A full debug target is about 8–10 GB (dependencies included). Remove an agent's directory when it is done;
`target/` itself had grown to 100 GB and about 950 000 files in `target/debug/deps`, which `cargo clean` (when
no one is building) resets.

### 2.4 One test binary per crate

Each crate's integration tests are one test binary, `it`, whose root is `crates/<crate>/tests/main.rs`: every
`tests/<name>.rs` is a module of it (`mod <name>;`), so a test's full name is `<name>::<test>` and nextest's id
is `<crate>::it <name>::<test>`. The files stay where they were, so `#[path]` includes, `include_str!`, the
`.proptest-regressions` files next to them and every path in these documents are unchanged. Support modules
used by more than one file (`support/*.rs`, `common/`, the benchmark workloads) are declared once in
`tests/main.rs` and used as `crate::<module>`; a file can't be compiled twice into one binary.

Each `Cargo.toml` sets `autotests = false` and names `it`, so a new `tests/<name>.rs` needs a `mod <name>;` in
`tests/main.rs`. `scripts/test.sh` refuses to run while one doesn't have it. The suites with a harness of
their own (`harness = false`: `k_corpus::corpus`, `k_server::runtime_js`, `k_server::corpus_run`,
`k_server::e2e`) stay binaries of their own.

```sh
cargo test -p k_sema --test it                    # every integration test of k_sema
cargo test -p k_sema --test it -- book::          # one file's tests (libtest: a name filter)
cargo test -p k_sema --test it -- book:: warm::   # two files (filters are alternatives)
cargo nextest run -p k_sema --test it book::      # the same with nextest
cargo nextest run -E 'binary_id(=k_sema::it) & test(/^book::/)'
```

A stack overflow aborts the process it happens in, so under `cargo test` it takes the rest of `it` with it;
nextest, the normal runner, runs every test in a process of its own.

Measured on the 14-core development Mac, warm build, other agents building at the same time:

| | ~90 binaries | One per crate |
|---|---|---|
| Test executables (integration + unit) | 87 + 10 | 14 + 10 |
| Their size in `target/debug/deps` | 2.29 GB | 0.70 GB |
| `scripts/test.sh` (warm build) | 25.1 s | 26.3 s |
| Tests (nextest and in-process, ignored ones included; doc tests apart) | 5647 | 5647 |

The run takes as long as before: nextest already ran every binary at once, and the slowest lane (Chrome,
the property tests) sets its length. What it saves is a link per binary after a change at the bottom of the
graph, macOS's first-run check per new executable (§2.1), a `--list` per binary on every nextest run, and
1.6 GB of disk per target directory.

---

## 3. Golden corpus

### 3.1 Layout

```text
tests/corpus/
  README.md
  lexer/                      ← suite directory
    README.md                 ← optional notes (.md files are ignored)
    strings/
      escapes.k               ← one case
      escapes.tokens          ← required expectation for the lexer suite
      escapes.diag            ← optional expected diagnostics
    numbers/units.k
    numbers/units.tokens
  parser/
    expressions/precedence.k
    expressions/precedence.ast
    errors/missing-paren.k
    errors/missing-paren.ast
    errors/missing-paren.diag
  sema/
    types/memberwise-missing-field.k       ← a single-file case
    types/memberwise-missing-field.diag
    types/memberwise-missing-field.pending ← expected to fail until the check exists
    modules/import-cycle/                   ← a project case: a directory with k.toml
      k.toml
      src/a/A.k
      src/b/B.k
      expected.diag
      expected.pending
```

- Every **direct subdirectory** of `tests/corpus` is a suite. An unknown suite name fails the run.
- `sema`, `codegen` and `run` also have **project cases**: a directory holding a `k.toml` is one case,
  and its expectations are `expected.<extension>` inside it. Extensions may contain a dot
  (`name.client.js`, `expected.server.js`); the longest matching extension of the suite wins.
- Every `*.k` file anywhere below a suite directory is **one case**. Subdirectories are only for
  grouping; nest them as deep as useful.
- Expectations are **siblings with the same stem**: `escapes.k` → `escapes.tokens`, `escapes.diag`.
- The test name is the path relative to `tests/corpus` (`lexer/strings/escapes.k`), so cargo's filter
  selects groups by directory.
- Layout mistakes are failing tests named `layout/<path>`: an expectation file without its `.k` file, an
  expectation extension the suite does not use (`.ast` in `lexer/`), or any other file type except `.md`.

| Suite | Runs | Required | Optional |
|---|---|---|---|
| `lexer` | `k_syntax::lex` | `name.tokens` | `name.diag` (lexer diagnostics) |
| `parser` | `k_syntax::parse` | `name.ast` | `name.diag` (lexer + parser diagnostics) |
| `sema` | `k_sema::Analysis` over a whole project | — | `name.diag` (every diagnostic), `name.pending`; projects: `expected.diag`, `expected.pending`; caret annotations in the sources (§3.8) |
| `codegen` | `k_codegen::generate` (profile `Test`) | at least one of `name.client.js`, `name.server.js`, `name.css`, `name.expect` | `name.pending`; projects: `expected.*` (§3.9) |
| `run` | `k_codegen::generate` + V8 (`k_server`) | — | `name.stdout`, `name.result`, `name.pending`; projects: `expected.*` (§3.10) |

**No `.diag` file means "no diagnostics".** Every diagnostic the stage produces must be listed.

Every lexer and parser case also checks losslessness: the token texts (lexer) or the tree text (parser)
must reproduce the source exactly.

### 3.2 Writing a case

1. Pick the suite and a directory for the topic (`lexer/strings/`, `parser/statements/`, ...).
2. Name the file after the behavior it pins down, in kebab-case: `unterminated-at-eof.k`,
   `trailing-lambda-in-if-condition.k`. One behavior per file; small files.
3. Write the source. **Mind the final newline**: most editors add one, and it appears in `.tokens` as
   `Newline "\n"`. A case about end-of-file behavior must *not* end with a newline (check with
   `tail -c1 file.k | xxd`).
4. Write the expectations by hand from `docs/compiler/grammar.md` (and `diagnostics.md` for spans). Do not
   bless output you have not reviewed.
5. Run `cargo test -p k_corpus --test corpus -- <your path>`. Before the stage is implemented, new cases
   fail; that is expected in test-first work.

Cover edge cases systematically for each feature: the empty form, the smallest valid form, every optional
part present and absent, maximal nesting, trivia (comments, blank lines, CRLF) in every position, the
end of file in the middle of the construct, and each diagnostic with its fix.

### 3.3 `.tokens` format (lexer suite)

One token per line: the `SyntaxKind` name, one or more spaces, and the token text as a quoted string.

- `Whitespace` tokens are **omitted** (listing one is an error). All other tokens are listed, including
  `Newline`, `LineComment`, `BlockComment`, `DocComment` and `Error`.
- Quoting uses `\\`, `\"`, `\n`, `\r`, `\t`, and `\u{HEX}` for other control characters; all other
  characters, including non-ASCII ones, are written literally.
- Blank lines and lines whose first non-blank character is `#` are ignored; use `#` lines for notes.
- Contextual keywords (`component`, `page`, `view`, ...) are `Ident` at the token level.

Example, `tests/corpus/lexer/strings/interpolation.k` containing `let a = "x{y}" // hi` and a final
newline:

```text
# interpolation splits the string into pieces
LetKw "let"
Ident "a"
Eq "="
StringStart "\""
StringText "x"
InterpStart "{"
Ident "y"
InterpEnd "}"
StringEnd "\""
LineComment "// hi"
Newline "\n"
```

The token kind names are listed in `crates/k_syntax/src/kind.rs` and described in
[grammar.md §3](grammar.md#3-lexical-structure).

### 3.4 `.ast` format (parser suite)

One S-expression: the dump of the whole tree, starting with `(SourceFile`. The dump rules are in
[grammar.md §19](grammar.md#19-syntax-tree-dump-format). In short:

- `(NodeKind children...)` for every node, in source order;
- tokens are bare atoms of their text (`count`, `var`, `+=`, `16px`, `#3B82F6`), except trivia and the
  structural punctuation `( ) [ ] { } , ; : . ? @ -> =>`, string quotes and interpolation braces, which
  are omitted;
- string contents are one quoted atom with the decoded value: `(StringLit "Hello, " (Interpolation
  (NameExpr name)) "!")`.

Layout is free and `;` starts a comment, because the comparison is structural:

```lisp
; tests/corpus/parser/declarations/component-minimal.ast
(SourceFile
  (ComponentDecl component Counter
    (MemberList
      (FieldDecl (Annotation State) var count (PathType Int) = (Literal 0)))))
```

On failure the harness prints a diff of both trees in canonical layout.

### 3.5 `.diag` format (all suites)

```text
# Lines starting with # are comments; blank lines are ignored.
error[K0002] 1:9-1:10: unterminated string literal
error[K0104] 2:6-2:8
  fix "Replace `&&` with `and`"
    edit 2:6-2:8 "and"
warning[K3001] 7:5-7:12: effect reads no reactive values
  label 6:5-6:12: declared here
  note: an effect re-runs only when a value it read changes
  help: read a `@State` or `@Derived` value, or remove `@Effect`
```

**Header line** (not indented): `severity[CODE] SPAN` optionally followed by `: message`.

- `severity` is `error` or `warning`; `CODE` is `K` and four digits.
- `SPAN` is `startLine:startCol-endLine:endCol`. Lines and columns are **1-based**, columns count
  **Unicode scalar values** (not bytes, not UTF-16), and the end is **exclusive**. An empty span (an
  insertion point) has equal start and end: `3:14-3:14`.
- Omitting `: message` makes the message a wildcard. Prefer omitting it unless the wording is what the
  case tests; codes and spans are the contract, messages may be polished later.

**Detail lines** (indented by any whitespace) belong to the header above them:

| Line | Meaning |
|---|---|
| `label SPAN` or `label SPAN: message` | A secondary label |
| `note: text` | A note |
| `help: text` | A help line |
| `fix "message"` or `fix "message" maybe-incorrect` | A quick fix (machine-applicable unless marked) |
| `edit SPAN "replacement"` | One edit of the preceding fix (quoted like `.tokens` text) |

If a header has **no** detail lines, the actual diagnostic's labels, notes, helps and fixes are not
compared. If it has **any**, all of them are compared exactly, in this order: labels, notes, helps,
fixes.

**Matching:** the order of entries in the file does not matter; each expected entry must match a distinct
actual diagnostic, and every actual diagnostic must be matched.

Span example: in `let s = "abc` (line 1), `l` is column 1 and the `"` is column 9, so an unterminated
string is `error[K0002] 1:9-1:10`. In `é = 1`, `=` is column 3 although `é` is two bytes.

**File paths.** Any `SPAN` (in a header, a `label` or an `edit`) may be prefixed with a file path and a
colon: `src/shop/Cart.k:3:5-3:9`. Paths contain no spaces. In **multi-file** cases (`sema` projects)
every header names its file, relative to the project root; a label or edit without a path is in the
header's file. In single-file cases paths are omitted, except for places in other files, such as a
label pointing into the standard library: `label <stdlib>/k/core/List.k:12:5-12:9: declared here`.

```text
error[K1101] src/shop/Cart.k:3:8-3:20: `shop.Missing` doesn't exist
  label src/shop/Other.k:1:1-1:5: a similar name is declared here
  fix "Remove the import"
    edit 3:1-4:1 ""
warning[K1105] App.k:2:8-2:21
```

Where each code's primary span goes is specified in [diagnostics.md](diagnostics.md).

### 3.6 Blessing

`K_BLESS=1` makes failing cases rewrite their expectations from the actual output (canonical layout;
a `.diag` file is created when diagnostics appear and deleted when none remain) and pass. Use it only
after a deliberate change whose new output you have reviewed, then review the diff in git. Corpus
expectations are normally written by hand.

### 3.7 Example cases

A lexer error case, `tests/corpus/lexer/strings/unterminated.k` (with a final newline):

```k
let s = "abc
```

`unterminated.tokens`:

```text
LetKw "let"
Ident "s"
Eq "="
StringStart "\""
StringText "abc"
Newline "\n"
```

`unterminated.diag`:

```text
error[K0002] 1:9-1:10
```

A parser recovery case, `tests/corpus/parser/errors/symbolic-and.k`:

```k
let ok = a && b
```

`symbolic-and.ast` (the parser treats `&&` as `and` for recovery):

```lisp
(SourceFile (FieldDecl let ok = (BinaryExpr (NameExpr a) && (NameExpr b))))
```

`symbolic-and.diag`:

```text
error[K0104] 1:12-1:14
  fix "Replace `&&` with `and`"
    edit 1:12-1:14 "and"
```

### 3.8 The `sema` suite

The `sema` suite checks **everything `k check` reports** for a project, syntax and semantic diagnostics
together, with the lockfile checks of `k_codegen::project_diagnostics` (`schema.lock` and the
migrations, `routes.lock`; [sema.md §12.16](sema.md#1216-lockfiles)) for an app without other errors,
plus assertions about inferred types and resolved names. The lockfile cases are project cases in
`app/persistence/lock/` (migrations and `schema.lock`), `routing/lock/` (`routes.lock`),
`app/storage/` (`storage.lock` keys), `app/jobs/` (job payloads) and `rpc/lock/` (`rpc.lock`). The design of the stage is
[sema.md](sema.md); every semantic code, with its primary span and fix, is in
[diagnostics.md](diagnostics.md#semantic-codes).

**Three kinds of case:**

- **Workspace:** a project directory whose `k.toml` declares a `[workspace]`
  (`tests/corpus/sema/workspaces/`). Every member is analysed with the libraries it depends on
  (`k_sema::Workspace`), and the case's diagnostics are the merged `Workspace::report`, with paths relative
  to the case directory: `error[K8104] a/k.toml:6:1-6:25`, `error[K4023] shop/src/checkout/Checkout.k:5:27-5:35`.
  Caret annotations are checked in the analysis of the member owning the file. `stack/` mirrors
  `examples/stack` without errors; the other cases each produce one workspace or `remote` diagnostic.
- **Single file:** any `name.k` below `tests/corpus/sema` (outside a project directory). It is analysed
  as the only source file of a library project whose `k.toml` is
  `[package]` / `name = "corpus"` / `k = "0.1"`, at the project path `src/corpus/name.k`. So the module
  is `corpus` and the file's declarations are `corpus.X`. A kebab-case file name can't match a public
  declaration, so a case that needs `public` uses a **path directive** on its first line:

  ```k
  // path: src/shop/Cart.k
  public struct Cart {}
  ```

  `// path: App.k` makes the file the app declaration. The directive is an ordinary comment, so it is
  line 1 of the file and counts in spans.
- **Project:** a directory containing `k.toml`. Every file below it is a project file at its relative
  path (`App.k`, `src/shop/Cart.k`, `migrations/0001_init.k`, `routes.lock`), with two exceptions in the
  directory itself: `expected.diag` and `expected.pending` are the case's expectations. The test name
  ends with `/` (`sema/modules/import-cycle/`). Nothing inside a project directory is a separate case.

**Expectations:**

| File | Meaning |
|---|---|
| `name.diag` / `expected.diag` | Every diagnostic, in the `.diag` format of §3.5. Absent means none. In a project every header names its file. Every code must be registered in `crates/k_diagnostics/src/codes.rs` (the case fails otherwise, even when pending). |
| `name.pending` / `expected.pending` | The case is **test-first**: it is expected to fail until the check exists. One line saying what is missing (`type inference (sema.md §9)`); empty is an error. |

**Pending cases** pass while their expectations are unmet, and **fail once everything holds**, with a
message to delete the `.pending` file. They must still parse exactly as intended: their syntax
diagnostics (K0001–K0999) are compared even while pending, so a typo in a test-first source fails at
once. Pending cases are never blessed. When you implement a check, delete the `.pending` files of its
cases and run the suite: each must now pass as a regular case.

**Caret annotations** assert what the position queries answer. They are comments, so they never change
the program; they do count as lines in spans.

```k
func total(items: List<Item>) -> Money {
    let prices = items.map { item => item.price }
    //  ^^^^^^ type: List<Money>
    //           ^^^^^ resolves: param items 1:12
    //                                    ^^^^^ resolves: member corpus.Item.price
    return prices.sum()
}
//@ 1:6-1:11 type: (List<corpus.Item>) -> Money
```

- A **caret line** is `//`, optional spaces, a run of `^`, then `kind: value`. The carets mark columns of
  the **target line**, the nearest line above that is not itself an annotation; the range runs from the
  first caret's column to just past the last caret's. Several annotation lines may share a target.
- An **explicit line** is `//@ SPAN kind: value`, with `SPAN` as in `.diag` files, for ranges that start
  in the first two columns or span several lines.
- The **range must be exactly a node's range**, trivia excluded: an expression, a pattern binding, a
  declaration's name, a member name after `.`, an element or attribute name, a CSS property, a class or
  keyframes name, a token name.
- `value` is `none` to assert that the query answers nothing there.

| Kind | Query | Value format |
|---|---|---|
| `type:` | `Analysis::type_at` | K type syntax ([sema.md §2](sema.md#2-public-api)): prelude types by simple name, others fully qualified: `List<shop.CartItem>`, `String?`, `(Int) async throws k.http.HttpError -> Bool`, `{error}` |
| `resolves:` | `Analysis::resolve_at` | one of the forms below |
| `side:` | `Analysis::side_of` (on a top-level declaration's name) | `server-only`, `client-only`, `shared` |

| `resolves:` value | Meaning |
|---|---|
| `decl shop.cart.CartService` | a top-level declaration (prelude ones by simple name: `decl Int`) |
| `member shop.Cart.add(product:)` | a member: field, function (with labels), case, computed or generated member (`member Page.product`) |
| `local items 3:9` | a local binding, with the line:column of its binding name |
| `param id 2:14` | a function parameter, with the position of its declaration |
| `element button` / `attribute button.type` | a built-in element / one of its attributes |
| `event button.click` / `event shop.Counter.changed` | an event in `.on(name:)` |
| `css padding` | a CSS property |
| `class shop.Summary.muted` / `keyframes shop.Toast.fade` | a class / keyframes of a component's style block |
| `token Spacing.md` | a theme token, as a member of its category |

A malformed annotation (unknown kind, empty value, carets past the end of the target line, carets on
line 1, an explicit span outside the file) fails the case, pending or not.

**Writing a sema case:**

1. Find the rule in [sema.md](sema.md) and its code in [diagnostics.md](diagnostics.md#semantic-codes)
   (primary span and fix). A rule without a code yet is deferred; don't write cases for it.
2. Put the case under the area's directory: `modules/`, `names/`, `types/`, `effects/`, `control-flow/`,
   `components/`, `views/`, `styles/`, `themes/`, `di/`, `routing/`, `rpc/`, `forms/`, `config/`,
   `persistence/`, `jobs/`, `tests/`, `docs/`, `clean/` (programs that must check cleanly).
3. Keep each case to one rule, with the smallest program that shows it. Name it after the behavior:
   `top-level-case-wildcard.k`, `optional-field-may-be-omitted.k`.
4. Write the `.diag` by hand. Cover the error and its near misses: the valid neighbour of an invalid
   program usually belongs in the same file, so the case also proves the rule doesn't over-fire.
5. If the check doesn't exist yet, add the `.pending` file. Run
   `cargo test -p k_corpus --test corpus -- sema/` and `k debug sema <case>` to see the current output.

### 3.9 The `codegen` suite

The `codegen` suite pins what the code generator emits ([codegen.md](codegen.md)). Cases are single files
or projects exactly as in the `sema` suite (§3.8: the implicit `corpus` library project, path directives,
`expected.*` inside project directories); `crates/k_corpus/src/codegen_suite.rs` implements it.

| File | Meaning |
|---|---|
| `name.client.js` | the client bundle **without** the runtime chunk (files whose path starts with `runtime.`), rendered as below |
| `name.server.js` | the server bundle, rendered as below |
| `name.css` | the stylesheets, rendered as below |
| `name.expect` | assertions about the three rendered outputs, for cases that pin a property rather than exact text |
| `name.pending` | test-first: one line saying what is missing; the case passes while it fails and fails once it passes |

At least one of the first four is required. **Generation options**: `Profile::Test` (readable output,
placeholder hashes `[hash]` in file names, readable class names `Component__class`, no source-map comments)
and `Environment::Production`, entry `App`.

**Rendering**: the files of one output kind, sorted by path, each under a header line and normalized:

```text
// file: shop/cart.js
export function CartBadge(props, ctx) {
  …
}
// file: pages/product.[hash].js
…
```

CSS files use `/* file: app.[hash].css */`. **Normalization** (applied to both sides): `\r\n` → `\n`,
trailing whitespace removed from every line, trailing blank lines dropped, one final newline.

**`.expect` format**, one assertion per line; `#` comments and blank lines are ignored:

```text
# The server-only repository and the secret's variable never reach the browser.
client lacks NoteRepository
client lacks STRIPE_SECRET
server contains NoteRepository
css contains .CartSummary__panel
```

`TARGET contains TEXT` / `TARGET lacks TEXT`, where `TARGET` is `client`, `server` or `css` (the rendered
outputs above), or `root` (the files served at the site's root, `Output::root`: the service worker `k-sw.js`
of [codegen.md §19](codegen.md#19-output-layout-of-dist)), and `TEXT` is everything after the second space,
matched literally.

**Always checked**, pending or not: the case must analyse **without errors** (a test-first case is still
a valid program), and `k_codegen::check_client_bundle` must find no server code in client files.
**Behaviour-only assertions** use the `run` suite (§3.10): a codegen case pins text, a run case pins what
the text does. With `K_BLESS=1`, a failing non-pending case rewrites the exact-output files it already
has; it never creates new ones and never touches `.expect`.

### 3.10 The `run` suite

The `run` suite is the **main behavioural suite for language semantics**: each case is compiled and
executed in V8, and its log output and result are compared. `crates/k_corpus/src/run_suite.rs` defines
it; the executor needs V8, so the cases run in `cargo test -p k_server --test corpus_run`
(`k_corpus`'s own corpus binary skips them, but still checks their layout).

- **Single file** `name.k`: the implicit `corpus` library project of §3.8; the entry is `corpus.main`.
  **Project**: a directory with `k.toml`; the entry is its one top-level function `main`.
- `main` takes no parameters and may be `async` and `throws`. It is generated with
  `Profile::Development` (so `Int` overflow checks are on) and `Environment::Test`, entry
  `Entry::Main(…)`: the server entry module exports `run()`, which calls `main` and returns the result
  line. Each case gets a fresh isolate and a 10-second limit.

| File | Format | Absent means |
|---|---|---|
| `name.stdout` | one line per `Log.*`/`console.*` call: `LEVEL MESSAGE`, `LEVEL` one of `debug`, `info`, `warning`, `error`; a line break in a message is written `\n` (backslash, `n`) and a backslash as `\\` | no output |
| `name.result` | exactly one line: `returns void`, `returns JSON`, `throws JSON` or `panics MESSAGE` | `returns void` |
| `name.pending` | test-first, as everywhere | — |

- `returns JSON` is the return value in its `Codable` JSON encoding (the wire mapping of
  [28-rpc-wire-format.md](../design/28-rpc-wire-format.md)); `main`'s return type must be `Codable` or
  `Void`.
- `throws JSON` is the uncaught typed error as `{"error": "corpus.ParseError", "case": "empty", "fields":
  {}}`.
- `panics MESSAGE` is the panic message exactly (`panics integer overflow: 9007199254740992 is outside the
  Int range`).
- JSON is compared as values, so spacing and key order don't matter.

```k
// tests/corpus/run/control-flow/match-guards.k
import k.observe.Log

enum Shape {
    case circle(radius: Int)
    case square(side: Int)
}

func describe(shape: Shape) -> String {
    return match shape {
        case .circle(let r) where r > 10 { "big circle" }
        case .circle { "circle" }
        case .square(let s) { "square {s}" }
    }
}

func main() -> List<String> {
    Log.info(describe(shape: .circle(radius: 20)))
    return [describe(shape: .circle(radius: 1)), describe(shape: .square(side: 2))]
}
```

`match-guards.stdout`: `info big circle`; `match-guards.result`: `returns ["circle", "square 2"]`.
Like codegen cases, run cases must analyse without errors, pending or not.

### 3.11 The `db` suite

`tests/corpus/db` holds **apps that persist**: each case is a project directory (a single `.k` file is a
layout error) with `k.toml`, `App.k`, `src/`, `migrations/`, optionally `schema.lock`, a `.env` setting
`DATABASE_URL=sqlite::memory:`, and a `requests.http` script in the format of §8.2. The suite is defined
in `k_corpus` (expectation: `expected.pending` only) and executed by
`cargo test -p k_server --test e2e -- db/`: the case is analysed (it must check cleanly), compiled, served
with a fresh database that the server migrates before the first request, and the script runs against it.
With `K_TEST_POSTGRES_URL` set, every case runs a second time as `db/<case>/postgres`, in a fresh Postgres
schema dropped afterwards.

The cases cover what [db.md](db.md) specifies end to end: `notes/` (queries of every shape, ordering,
`including`, bulk writes, `@Version`, `insert(all:)`, `Clock`), `accounts/` (typed unique violations,
rollback, savepoints, a primary-key panic, `ping`), `catalog/` (money, decimals, enums, optionals, a
one-field id, `LIKE` escaping, `IN` lists, nil equality, bulk arithmetic, locked names, a data migration
and `renameColumn`). Ids are chosen by the client so later exchanges can refer to them.

---

## 4. Unit and integration tests

- **Unit tests** live in a `#[cfg(test)] mod tests` at the bottom of the file they test. Name tests after
  the behavior: `offsets_past_end_are_clamped`, not `test_line_index_3`.
- **Integration tests** live in `crates/<crate>/tests/*.rs` and use only the public API. Start each file
  with a `//!` comment saying what it covers, and add `mod <name>;` for it to the crate's `tests/main.rs`
  (§2.4).
- Tests may `unwrap`, `expect` and `panic` (clippy allows it in tests). Production code may not.
- Build syntax trees by hand in unit tests with `rowan::GreenNodeBuilder` (see `crates/k_syntax/src/ast.rs`
  tests) when a test should not depend on the parser.
- **Grammar coverage against real code:** `crates/k_syntax/tests/examples.rs` parses every `.k` file of
  the example apps in `examples/` and fails on any syntax diagnostic. The examples are written against
  `docs/design`, not the parser, so a construct the design adopted but the grammar lacks fails here.
  When it does, add the construct to `grammar.md` and the corpus first, then to the parser.

### 4.1 Runtime JS tests

The JavaScript runtime ([runtime.md](runtime.md)) is tested **inside V8**, never in Node:
`crates/k_runtime/js/tests/*.test.js` are ES modules run by `crates/k_server/tests/runtime_js.rs`.

```js
// crates/k_runtime/js/tests/signals.test.js
import { assertEq, assertDeepEq, assertThrows, installDom, test, tick } from "k:testing";
import { batch, renderEffect, root, signal } from "k:runtime";

test("batch flushes synchronously when the outermost batch ends", () => {
  const a = signal(1);
  const seen = [];
  root(() => renderEffect(() => seen.push(a.get())));
  batch(() => a.set(2));
  assertDeepEq(seen, [1, 2]);
});

test.pending("each reorders keyed rows by moving nodes", "keyed lists (runtime.md §5.3)", () => { … });
```

- **Fixtures**: a `name.json` next to the test files is importable as `k:/tests/name.json.js`, whose default
  export is the parsed value; the channel frame vectors (`channel-frames.json`) use it so the Rust tests
  (`crates/k_server/tests/channels.rs`) and the JS tests read one file.
- **Registering**: `test(name, fn)` (`fn` may be async) and `test.pending(name, reason, fn)`; names are
  unique per file. Each registered test is two cargo tests: `<file>/<name>` against the runtime as written,
  and `<file>/<name> (compacted)` against the runtime after `k_runtime::compact`, which is what release
  builds ship (`EngineOptions::compact_runtime`), so a compaction that changes what the runtime means fails
  here. `cargo test -p k_server --test runtime_js -- signals.test` runs one file.
- **Server tests**: `server-app.test.js` drives `k:runtime/app` with hand-written specs; the isolate has
  K's ops, so sessions, rate limits and `.server` state work as in a server (each test's isolate has a
  fresh, private process state). `outbound.test.js` drives the server's I/O services
  (`FetchHttpClient`, `SmtpEmailProvider`, the job queues) over stand-ins for their ops, passed as
  their `transport` arguments; the ops themselves are tested in Rust (`tests/outbound.rs`).
- **Isolation**: every test runs in a **fresh isolate**, with `installDom()` already called (a fresh fake
  `document` and the DOM classes on `globalThis`), and a 10-second limit.
- **Assertions** from `k:testing`: `assert(v, msg?)`, `assertEq(actual, expected, msg?)` (`Object.is`),
  `assertDeepEq` (JSON-equal, `Map`/`Set` aware), `assertThrows(fn, substring?)` (returns the error),
  `tick()` (lets microtasks and effect flushes run).
- **Pending**: a `test.pending` test passes while it fails and fails once it passes, with a message to
  turn it into `test`, exactly like `.pending` corpus files.
- **The fake DOM** (`k:testing/dom`, [runtime.md §13](runtime.md#13-testing)) parses compiler-style HTML
  and throws on malformed input. It has focus (`focus()`, `blur()`, `document.activeElement`) and boxes
  only where a test gives them: an element's `rect` (`{left, top, width, height}`) is what
  `getBoundingClientRect` and `document.elementFromPoint` read.
- **Feature chunks**: `drag.test.js` (pointer, touch with stubbed timers, the keyboard path and its
  announcements, the indicator, hydration, a fresh document per `k test` test), `drag-ssr.test.js` (the
  server's markup and the browser claiming it), `files.test.js` (the picker, its errors, downloads over
  stubbed `URL` and `Blob`), `keys.test.js` (`KeyboardShortcut`'s filters), and `clipboard.test.js`
  and `share.test.js` (`BrowserClipboard` and `BrowserShare` against scripted `navigator.clipboard`
  and `navigator.share` fakes with each engine's documented rejections; `CopyButton` and `ShareButton`
  with a recording container: the copy at the click, the two seconds on the clock, the events, the
  fallback copy, the server's markup and hydration), and `geolocation.test.js` (`BrowserGeolocation`
  against scripted `navigator.geolocation` and `navigator.permissions`: an insecure context, reads that
  never prompt, the prompt by one position request, each error code, Gecko's allow-once, no Permissions
  API, WebKit's missing `change` event, and watches ending on revocation and with their owner),
  `environment.test.js` (the environment services against a scripted `document.visibilityState`,
  `navigator.onLine`, `matchMedia`, `innerWidth` and `requestAnimationFrame`: each event, the back-forward
  cache's `pageshow`, a failed rpc taking the app offline until one succeeds or `online` fires, aborts and
  other origins not counting, resizes coalesced per frame, and the server's values while hydrating),
  `tabs.test.js` (`BrowserTabMessaging` against a scripted `BroadcastChannel`: another tab hears a post
  and the posting one doesn't, channels namespaced per app, another build's and undecodable messages
  dropped, cancelling, `messages` ending with its owner or a `for await` left early, no
  `BroadcastChannel`), `wakelock.test.js` (`KeepScreenAwake` against a scripted `navigator.wakeLock`: held
  while mounted, released when hidden and asked again when shown, a request resolving after unmount
  released, a refusal, no API or an insecure context, the container's `TestWakeLock`, the server's empty
  markup), `notifications.test.js` (`BrowserNotifications` against a scripted `Notification`,
  `navigator.serviceWorker` and `navigator.permissions`: an insecure context and a Safari tab on iOS
  unsupported, the permission after hydration and on `change` or (WebKit) on becoming visible, a show
  before asking `notAsked` with nothing registered, the worker registered once at `/k-sw.js` with scope
  `/` and `updateViaCache: "none"`, the page's URL in the notification's data, a refused and a quieted
  prompt, Gecko's private windows rejecting `register`, a permission revoked mid-show, and the worker's
  `open` message navigating and answered), `offline.test.js` (`startOffline` and `AppUpdates` against a
  scripted `navigator.serviceWorker`: registered after `load`, once, at `/k-sw.js`; nothing outside a
  secure context; the controlling worker's build compared with the page's at start and on
  `controllerchange`; an update check when the page becomes visible; `apply()` reloading),
  `sw-offline.test.js` (the caching half of the worker against a scripted `CacheStorage` and `fetch`:
  install, a failed install, a `no-store` shell, the shell and files offline, rpcs and other origins left
  alone, server-rendered pages kept and served offline, signed-in pages and errors not kept, 50 pages at
  most, two builds kept on activate, the build message, the development worker, one worker with the
  notification handlers), `workers.test.js` (`workerClient` against a scripted `Worker`: started at the
  first call and posted to once ready, one call at a time in order, typed errors, a queued and a running
  call cancelled (the running one terminating the worker), an aborted signal, a crash restarting it, an
  `error` before and after ready, no `Worker`, an undecodable answer; `serveWorker` against a scripted
  scope; the two through a pair of ports; docs/design/55-workers.md), `sw.test.js` (the service worker in a scripted
  `ServiceWorkerGlobalScope`: `skipWaiting` and `clients.claim`, a click focusing the focused tab, else
  the most recent, and posting it the page, the worker navigating a tab that doesn't answer within a
  second or opening a new tab when it can't, a notification without a page only focusing, no tab open,
  and a refused `focus`) and
  `observe.test.js` (the element events against scripted observers: entries and exits, thresholds, one
  observer per option set, sizes, and disposal) and `media.test.js` (`k.media` against scripted
  `mediaDevices`, `permissions`, `MediaRecorder` and `Blob`: an insecure context, `.notAsked` without
  calling the browser, the held stream a capture takes over, each `CaptureError` mapping, owners
  stopping streams, recordings, the controller and the `video` and `audio` binding) test the chunks of
  [runtime.md §12.3–12.12](runtime.md#123-drag-and-drop).
- **Cancellation**: `cancellation.test.js` pins the ownership of async work (design 03, "Async work is
  owned"): `live` and `CANCELLED`, a body stopped at an `await` (and a `catch`) whose owner was disposed
  after the promise settled, an effect re-run and a load stopping their superseded run, `on` keeping a
  stopped handler silent, and `own`. `ui-router.test.js` adds a guarded page whose handler awaits an rpc
  while another rpc's response ends the session. Generated pages are driven the same way by
  `crates/k_server/tests/cancellation.rs`: the client of `tests/projects/cancellation` runs in V8 with
  the fake DOM and a scripted `fetch`, and each case (the session ending while a guarded page's handler
  awaits, at every gap between the two responses; leaving a page mid-await; an effect re-run; an async
  `@Emits` handler and an inline `form.handle` action) checks that no code after an `await` ran on the
  page that was left, that its requests were aborted, and that nothing was reported.
- The driver protocol: it evaluates `import { list } from "k:testing"; import "k:/tests/<file>";` to get
  `[{name, pending}]`, then per test `run(name)`, which returns `{"outcome": "passed"}` or
  `{"outcome": "failed", "message": …}` through `op_k_set_result`.

### 4.2 Database tests

`crates/k_db/tests/drivers.rs` runs **every test against every available database**
([db.md §13](db.md#13-testing)):

```rust
#[tokio::test]
async fn duplicates_map_to_the_unique_field() {
    for t in k_db::testing::test_databases().await.expect("databases") {
        // … use t.db; label assertions with t.name ("sqlite" / "postgres") …
        t.finish().await.expect("cleanup");
    }
}
```

- **SQLite** (in memory) always; **Postgres** when `K_TEST_POSTGRES_URL` is set, each test in a fresh
  schema `k_test_<pid>_<n>` that `finish` drops. Unset, Postgres is skipped silently and nothing fails; set
  but unreachable, the test fails, because the run asked for Postgres.
- A local Postgres for a run: `docker run --rm -e POSTGRES_PASSWORD=pw -e POSTGRES_DB=k_test -p
  127.0.0.1:55432:5432 postgres:17-alpine`, then
  `K_TEST_POSTGRES_URL=postgres://postgres:pw@127.0.0.1:55432/k_test cargo test -p k_db`.
- SQL text is tested separately and exactly (`crates/k_db/tests/sql.rs`), without a database, and so is
  the migration builder's DDL (`builder.rs`, which then applies it to each database).
- The same `K_TEST_POSTGRES_URL` runs the database corpus on Postgres too (§3.11).

### 4.3 Browser tests

`crates/k_server/tests/browser.rs` runs the example apps as a browser would, without a browser:

- **The site**: the example is analysed and compiled for development (`Environment::Development`), and
  served by an in-process `k_server::Server` with its client assets, its configuration given in place of
  a `.env`, and a fresh SQLite file migrated at startup (seeded with `scripts/demo-catalog.sql` for
  `shop`).
- **The tab** (`tests/browser/sim.js`, an engine of its own): the fake DOM of `k:testing` plus the globals
  a K client reads (`location`, `history`, `localStorage`, `document.cookie`, `matchMedia`,
  `crypto.randomUUID`, fake timers that fire only on `sim.advance(ms)`). `Tab::open` fetches the page from
  the server, puts its `<body>` into the DOM, records the container's element nodes and markup, and
  imports the generated client entry, which hydrates it.
- **Requests**: same-origin `fetch` calls queue in an outbox; the driver forwards them over HTTP with the
  tab's cookies (and `Origin`/`Sec-Fetch-Site`, as a browser sends them), delivers the responses and
  repeats until nothing is in flight, keeping the cookies the server sets. Other origins are answered by
  a scripted fake (`sim.external`, the Open-Meteo fake of `weather`). A **server-streaming rpc**
  (`application/connect+json`, its envelope sent as bytes) is opened with `k_server::testing::open_stream`
  and stays open: the tab gets the response head as a streamed response, and every later `settle` hands
  it the body pieces the server has written (`sim.deliverChunks`; the first piece of a new stream is
  awaited, so its opening message is in the page). A stream the client stops reading (its effect was
  cancelled, a `for await` left) is reported by `sim.drain` and its connection closed;
  `Tab::wait_for_text` settles until a change that reaches the tab on its own (a stream's element) shows.
- **The server's third parties**: `ServerOptions::fetch_origins` sends the server's outgoing requests to a
  fixed third-party origin to a `FakeHttp` instead (`weather`'s Open-Meteo: a place, a forecast in Berlin's
  zone, air quality; or a 404 for an unknown place), so a page that loads through the server's
  `HttpClient` renders on the server and hydrates in the tab.
- **Assertions**: `assert_hydrated` fails when hydration replaced any server node (a mismatch) or changed
  the markup, as of the end of the pass (the sim snapshots the page when the runtime sets `k_tz`, right
  after the pass, so a client-rendered component's later swap to its `view` doesn't count; the
  `tests:client-component` project checks that swap and the ids after it); the driver's request log shows that `@Load` data isn't fetched again on hydration; the
  container's text and markup after each interaction; no `console.error` and no uncaught exception.
- **Cases**: `todos` (client rendering: add, a refused blank title, toggle, the three filters, delete,
  the theme switch with its root class and cookie, a reload from local storage), `weather` (hydration,
  a debounced search through `FetchHttpClient` against the fake, results, no results; a city page rendered
  on the server with its `LineChart` in the forecast's time zone and hydrated without fetching again; a
  client navigation to a city that keeps the shared layout mounted and updates `document.title`, and back;
  an unknown city's 404 hydrated into its `failed` block), `notes` (a wrong password, sign in, a new note
  from the toolbar link, a note created and then deleted elsewhere reaching the page through its open
  `watch()` stream, the stream closed when the page is left, editing the server-rendered note, and leaving
  the edit page while its delete awaits the server, which stops the handler before its navigation) and `shop`
  (the `shop` member of the `examples/shop` workspace, with paybox stood in for by a `FakeHttp`:
  storefront hydration, a client navigation to a product with one `Product` rpc, the order form's
  validation without an rpc, placing an order, which registers the shop with paybox under a service
  token, the card form's validation without an rpc, paying, which the shop's server charges through
  paybox, and the receipt's polling).
- **The shipped build** (`crates/k_cli/tests/dist_browser.rs`): the same tab and driver against what
  `k build` ships. Each example is built with the `k` binary, its `dist/app serve` runs as a process on a
  free port (a fresh SQLite file for `notes` and `shop`, placeholder secrets), the tab loads the release
  client files of `dist/public/assets`, and the driver forwards its requests to the process over HTTP. A
  static-only example (`todos`, `kanban`) has no `dist/app`: its `dist/public` is served by the static
  host of `tests/support/static_host.rs`, which behaves like the examples' nginx images (immutable
  `/assets/`, a `no-cache` `index.html` for every other URL).
  Cases: `todos` (add a todo, `BrowserStorage`, the `Theme` root class and cookie), `kanban` (create a
  board, which navigates to `/boards/<id>`; add cards with the N `KeyboardShortcut`; move one with Space,
  ArrowRight and Space and read the live region's announcement; move one with pointer events; the chart;
  both moves in local storage; the theme; and a board reopened from saved storage at its URL), `weather` (hydration,
  a search debounced on the injected `Clock` through `FetchHttpClient`), `notes` (sign in, the `Session`
  mirror in the layout, a new note, a note saved elsewhere arriving through the `watch()` stream, editing
  the server-rendered note without refetching it) and `shop`
  (storefront hydration, a client navigation, the order form's validation in the browser, a missing
  product's 404 rendered by its `failed` branch, the sign-in page, and no secret value, service token
  header or server-only or production-unbound service name in any client file or response). `dist/app`
  must exit 0 on `SIGTERM` and log no errors.
- **The client guards in the shipped build**: the tab records full page loads (`location.href`) and
  pushed history apart, and `sim.traverse(path)` stands in for Back and Forward (the URL changes, then
  `popstate`). `notes`: a guest's `Router.navigate` (the runtime's `navigate`, by path and with the origin)
  and Back or Forward to a guarded entry are each one full load of exactly that URL with nothing rendered,
  and the router renders nothing after leaving; signing out on a guarded page, and a session that
  expires while one is shown (found by its next rpc), load the URL shown, which the server answers with
  the `303` and the return URL; a guarded document is `private, no-store`. `shop` (with an Argon2id
  admin hash made by the runtime's own op): a guest's navigation to `/admin`, signing in and continuing
  there on the client, then signing out, an expired session and a role taken away while the page is
  shown, each leaving for the server (`303` without a user, `403` without the role), the page's own
  `Router.navigate(to: .home)` ignored.
- **A shop order through a real paybox**
  (`shop_dist_app_places_and_pays_an_order_through_a_paybox_dist_app`): both apps of the `examples/shop`
  workspace built with `k build` and run as two `dist/app` processes with different service secrets, as
  in its `docker-compose.yml`, and a `FakeSmtp`. The tab places an order, which registers the shop with
  paybox (paybox's `merchant_account` names the shop by its token) and goes to the card form; the
  decline card is refused and charges nothing; the success card is charged through paybox, and the tab
  goes to the receipt. Paybox's `EventDeliveryJob` then calls the shop's `PaymentEvents` signed with the
  shop's secret: the order is paid with paybox's charge id, paybox's job queue empties, the receipt turns
  to "Payment received" on its next reload, and exactly one confirmation is sent over SMTP. No service
  secret reaches a response.
- **On Postgres** (`shop_dist_app_places_and_pays_an_order_on_postgres_when_a_postgres_url_is_given`):
  the same order with the shop on Postgres when `DATABASE_URL` is a `postgres://` URL, in a database the
  test creates and drops (`CREATE DATABASE` rights needed); skipped, with a note on standard error,
  otherwise.
  `examples/shop/docker-compose.test.yml` starts a throwaway server on `127.0.0.1:55432`:

  ```text
  docker compose --file examples/shop/docker-compose.test.yml up --detach --wait
  DATABASE_URL=postgres://k_test@127.0.0.1:55432/k_test cargo test -p k_cli --test it -- dist_browser::shop_dist_app_places_and_pays_an_order_on_postgres
  ```
- `K_DUMP=<example> K_DUMP_DIR=<dir> cargo test -p k_server --test it -- browser::dump` writes the generated
  files of an example, for debugging; `K_E2E_LOG=1` prints the server's log lines.

### 4.4 Real-browser tests

The simulated tab of §4.3 has no layout, no CSS cascade, no fonts, no focus and no default actions (a
form's submission, a link's navigation), so a page can pass there and still be broken for a person: a
title that only `aria-label` carries, a form that reloads the page, a theme that never reaches the
document. `crates/k_cli/tests/chrome.rs` runs every example in **headless Chrome** instead, as its
README runs it, and keeps §4.3 for what a fake DOM does well (hydration markers, request counts, fake
timers).

- **The browser**: Chrome at `K_CHROME`, else the usual install locations
  (`/Applications/Google Chrome.app/…`, `/usr/bin/google-chrome`, `/usr/bin/chromium`, …), started per
  test with `--headless=new` and a throwaway profile below Cargo's `CARGO_TARGET_TMPDIR`. Without one,
  each test prints `skipped: no Chrome found …` and passes; with `K_REQUIRE_CHROME=1` (CI) a missing
  Chrome fails it.
- **Media** (46-media-and-rtc.md): the media tests start their Chrome with
  `--use-fake-device-for-media-stream` (and `--use-fake-ui-for-media-stream` where every prompt should be
  answered "Allow"), and map `insecure.test` to loopback for an `http` origin that isn't a secure
  context. They provoke `.notAsked`, `.denied` (`Browser.setPermission`) and `.unsupported` without a
  live stream; the parts that need one (`.notFound`, recording each named format, loading it in a
  `video` element and decoding its audio to about two seconds, the memos walkthrough) first check that `getUserMedia` answers within 5 s. On macOS
  Chrome asks the system for the camera and microphone even when they are fake, and a test process that
  was never granted them waits forever: those parts then print `skipped: …` and pass, unless
  `K_REQUIRE_MEDIA=1` makes them fail.
- **Calls** (46-media-and-rtc.md): the `rtc_*` tests open two tabs of one Chrome on
  `tests/fixtures/calling` under `k dev`, with the fake camera allowed, background tabs kept running and
  host candidates as addresses (not `.local` names), and send **video only**, since the fake camera opens
  on macOS where the fake microphone waits. They call with no ICE servers; through a `coturn/coturn:4.6`
  container (Docker; skipped without it unless `K_REQUIRE_DOCKER=1`) with `IcePolicy.relayOnly`, checking
  that `stats()` reports the relay; under glare forced by holding both tabs' WebSocket frames while each
  adds its camera; and across a mid-call `replace`, counting descriptions through a wrapped
  `RTCPeerConnection`. coturn's port is published on every interface and the TURN URL names this
  machine's network address: Docker Desktop here doesn't forward a port published on `127.0.0.1` alone,
  and Chrome reaches a TURN server only from a network interface. The ICE restart after a network
  namespace drops packets isn't run: macOS has no network namespaces.
- **Workers** (55-workers.md): the `a_worker_*` tests serve `tests/fixtures/workers` under `k run` and
  count primes in its `@Worker` service. While a count that takes seconds runs, the page must paint at
  least 20 `requestAnimationFrame` frames in a second (a count on the main thread paints none); stopping
  it must let the next count answer within 5 s from a new worker; a refused count shows its typed error's
  payload; a crash shows `.workerFailure(.crashed)` (its `console.error` excused) and the next count still
  answers. `notes_ranks_the_filtered_notes_in_a_worker_in_chrome` checks the notes filter's ranking and
  that the page loaded `/assets/workers/note-search.<hash>.js`.
- **Firefox** (`tests/firefox.rs`, `tests/support/bidi.rs`): the stdlib's own bindings run on a second
  engine, headless Firefox driven over **WebDriver BiDi**, with the checks written once in
  `tests/engines.rs` against a small `Engine` trait (open, reload, evaluate, a trusted click, set a
  permission, emulate a position) that `page::Tab` and `bidi::Firefox` both implement; `chrome.rs` runs
  the same bodies. The shared checks are the crypto vectors on Web Crypto and the differential against
  the server's `ops` backend (33-crypto.md, "Testing"), the `k.device` bindings (`Clipboard`, `Share`,
  `Geolocation`, `Notifications`, `TabMessaging`; 45-web-platform.md, "Tests"), imported from
  `crates/k_runtime/js/runtime` as written, and capturing (`.notAsked`, `.denied`, `.unsupported`,
  `.notFound`) and recording every named format with fake devices on `tests/fixtures/capturing`
  (46-media-and-rtc.md, "Testing the stdlib against real browsers"). Where the engines are documented to
  differ, each engine's answer is asserted: desktop Firefox has no share sheet (`.unsupported`), and a
  Gecko clipboard write or read without a click is `.denied`. Firefox is found at `K_FIREFOX`, else the
  usual install locations (`/Applications/Firefox.app/…`, `/usr/bin/firefox`, …); without one each test
  prints `skipped: no Firefox found …` and passes, and with `K_REQUIRE_FIREFOX=1` (CI) a missing
  Firefox fails it. Each test starts `firefox --headless --no-remote` with a fresh profile whose
  `user.js` turns off updates, telemetry, studies, remote settings, safe browsing, captive-portal and
  connectivity checks, DNS over HTTPS and the new-tab feeds, and sends everything but loopback to a
  proxy on a closed port, so nothing the browser or the page does reaches the internet; it gives Firefox
  fake devices (`media.navigator.streams.fake`, and `media.navigator.permission.disabled` where every
  prompt should be allowed) and resolves `insecure.test` to loopback (`network.dns.localDomains`).
  `--remote-debugging-port` is a free port in 5200–5999 (Firefox writes no `DevToolsActivePort`); the
  driver connects to `ws://127.0.0.1:<port>/session` on the same WebSocket client as `cdp.rs` (BiDi's
  messages have the same `id`/`method`/`params` shape), starts `session.new`, drives the top-level
  context with `browsingContext.navigate`/`reload` (`wait: "complete"`), `script.evaluate` and
  `script.callFunction` (values cross as JSON text), `input.performActions` for clicks,
  `permissions.setPermission` and `emulation.setGeolocationOverride`, and subscribes to
  `log.entryAdded` and `network.beforeRequestSent`. Dropping it fails the test on a console error, an
  uncaught exception or a request beyond loopback (`firefox_reports_console_errors_and_keeps_the_page_off_the_internet`
  checks the driver sees all three), then kills Firefox and its content processes (`ChildGuard`, also
  on a panic) and removes the profile. The Firefox tests share the `chrome` nextest group.
- **The protocol client** (`tests/support/cdp.rs`): the DevTools protocol over a WebSocket, both
  written for the tests on `std::net` and `serde_json` (no new dependency: one loopback connection needs
  a masked text frame out and possibly fragmented text frames in). A reader thread forwards responses
  and events to the test, and answers `Fetch.requestPaused` itself, so a paused request never waits for
  a test blocked in a command. Chrome starts with `--remote-debugging-port=0` and the test reads the port
  from the profile's `DevToolsActivePort`, within 90 s (a dozen Chromes starting at once beside a full
  run took longer than the 30 s this was). A command that takes over 5 s is noted on standard error,
  which the harness shows for a failing test.
- **No internet**: every `https` request the page makes is intercepted. A test answers it (weather's
  Open-Meteo: a search, Berlin, a forecast, air quality), a third-party image gets a transparent pixel,
  and anything else fails the request and the test. The server's own requests go to a `FakeHttp` through
  `K_DEV_FETCH_ORIGINS` (below). Every port a test binds is in 5200–5999, skipping ports something
  already answers on.
- **Serving** (`tests/support/apps.rs`): a static-only example (`todos`, `kanban`) is built with `k build`
  and served by the static host of `tests/support/static_host.rs`; the others run under `k run` or `k dev`
  in a scratch copy of the example (so nothing is written to the repository), `k dev` at the root for the
  `stack` and `shop` workspaces. The shop needs no setup, as its README says: the development database,
  the generated admin password, the demo catalog and the mail catcher (`.k/mail/`) come from `k dev`.
  **Every example runs its release client**: `k run` and `k dev` get `--release` (`Proc::start`), so the
  browser runs the linked, minified client with short class names that `k build` ships
  ([codegen.md §17](codegen.md#17-bundling-chunks-and-printing)); `K_CHROME_PROFILE=development` serves
  the development client instead. Tests therefore find elements by role, visible text or attributes such
  as `data-icon`, never by class name.
- **The tab** (`tests/support/page.rs`): clicks and drags are mouse events at the element's on-screen
  centre, after checking that the element is what is on top there (an element replaced by a re-render is
  found again by its query); typing and named keys are key events (Enter submits a form as a person's
  Enter does). Queries read what is on screen: `role(role, name)` from Chrome's accessibility tree,
  `by_text` and `visible_text` from text that has a box and isn't clipped away (`VisuallyHidden`,
  `display: none`, `visibility: hidden`), unlike `textContent`. `style` reads computed styles,
  `used_fonts` the fonts Chrome actually drew with (`CSS.getPlatformFontsForNode`), `is_focused`
  `document.activeElement`, `document_token`/`assert_same_document` whether anything reloaded the page,
  and `reload` keeps cookies and storage (it is the same profile). A tab fails its test when dropped if
  the page logged a console error, threw, failed a load or reached for the internet. Waits poll a
  condition for up to 30 s (`WAIT`) and each DevTools query may take as long; a timed-out wait says
  what the page shows, why the last query failed, and whether the page is hidden. `has_no_role` is
  "the accessibility tree answered, without it", never a failed query. `wait_handles_click(el)` and
  `wait_handles(el, event)` wait until the page's script handles the click (or the form's submit) or
  the event: a server-rendered page shows its controls before hydration wires them. `Proc`s print
  their last output when a test fails. `K_CHROME_THROTTLE=4` slows every tab's CPU four times
  (`Emulation.setCPUThrottlingRate`), to shake out races.
- **Cases**: each example's main README flow, green: `todos` (add, complete, the three filters with
  `aria-pressed`, clear, a reload; "Remind me" with the notification permission granted by
  `Browser.grantPermissions`, the static build's `/k-sw.js` served as JavaScript, and the reminder shown
  through the `/k-sw.js` registration, time passing quickly through a script run before the
  page's with `Page.addScriptToEvaluateOnNewDocument`: each timer of a minute or more is held until the
  test has seen "Reminder set" and calls `__kFastForward()`, then fires at once and moves `Date.now` on by
  its delay, and each `showNotification` that resolved is recorded for the test.
  Not `getNotifications()`: on macOS it asks the system's notification center, which stalls past the
  test's wait when several Chromes ask at once; `@Offline`, docs/design/54-offline.md: once the worker
  controls the page, a reload with the static host stopped and `Network.emulateNetworkConditions`
  offline opens the app with its saved todos, and a rebuild into the served directory is found by
  `registration.update()`, shows "A new version of Todos is ready." and "Reload" opens it), `kanban` (create a board, add cards, move one with Space, ArrowRight
  and Space keeping the focus, drop one onto another with the mouse, Escape closing the editor but not
  while typing in it, a reload, "Copy link" putting the card's absolute link on the real clipboard, with the
  permission granted by `Browser.grantPermissions`, and the link opening the card's editor), `weather` (a debounced search, a client navigation to a result, °C/°F, home again;
  the recently viewed list, a client-rendered component, appearing after hydration without moving the
  server-rendered parts; a refresh after server rendering keeping the place; "Use my location", absent
  from the server's HTML, reading the position set by `Emulation.setGeolocationOverride` with the
  permission granted by `Browser.grantPermissions`, and opening `LocalWeather` there),
  `notes` (one command, sign up, create, edit, delete, sign out and in; two apps on `localhost` keeping
  their own sessions; `k dev` stopping promptly with a `watch()` stream open; the offline banner appearing
  under `Network.emulateNetworkConditions` offline and going when the network is back), `stack` (pay from the
  shop page through paybox) and `shop` (buy a mug, pay through paybox on non-default ports, the receipt
  turning paid, the confirmation in the mail catcher; two tabs of one browser, the second opened with
  `Target.createTarget` (`Tab::new_tab`), the cart added to in one updating the other's header in place,
  and emptied from the other). Besides the examples, `crates/k_server/tests/projects/channels` under `k dev`:
  its `Room` page opening a channel, a message and its echo, the socket closed from DevTools while new ones
  are refused (a spy installed before the page's scripts), and the channel resuming with an announcement
  published meanwhile, delivered exactly once. And one red test per open defect (§4.5).

`K_DEV_FETCH_ORIGINS` (comma-separated `from=to` origin pairs) is read by `k run` and `k dev` only: a
server-side `HttpClient` request to `from`, or below it, goes to `to` instead
(`ServerOptions::fetch_origins`, as §4.3 sets in process). `dist/app` and builds never read it.
`crates/k_cli/tests/example_checks.rs` tests it, and the examples' setup scripts and files (the shop's
`init-secrets.sh` without `argon2`, a config `@Matches` refused at startup, the compose file's SMTP port,
a member's `.env` watched by workspace `k dev`, the stack's ports, no server-only rpc client in a client
bundle, the notes example's ignored files, the READMEs).

```text
cargo test -p k_cli --test it -- chrome::
K_CHROME=/path/to/chromium K_REQUIRE_CHROME=1 cargo test -p k_cli --test it -- chrome::
cargo test -p k_cli --test it -- firefox::
K_FIREFOX=/path/to/firefox K_REQUIRE_FIREFOX=1 K_REQUIRE_MEDIA=1 cargo test -p k_cli --test it -- firefox::
```

### 4.5 Red tests

A test for a defect that is reproduced but not fixed yet is written first and committed ignored, with
the defect's id: `#[ignore = "red: <id> <what is wrong>"]`. The fix removes the `ignore` in the same
change, so the test turns green and stays. `cargo test -- --ignored` runs the red tests (each must fail,
for the reason its message names); `grep -n 'ignore = "red:' crates/*/tests/*.rs` lists them. The ids
are those of the real-browser diagnosis of the examples:

| Id | Defect | Test (`crates/k_cli/tests/`) |
|---|---|---|
| todos-A | **Fixed** (every form control `.label` is a `<label>`; codegen.md §10): a computed `.label` on a form control is `aria-label` only, not a visible `<label>` | `chrome.rs` `todos_shows_each_todos_title_on_screen` |
| todos-B | **Fixed** (`on` prevents a `submit`'s default; runtime.md §5.4; `sim.submit` fails a submission nothing prevented): `.on(submit:)` doesn't prevent the browser's submission, so adding reloads the page and a rejected title loses its alert | `todos_adds_in_place_and_keeps_the_focus_in_the_field`, `todos_rejects_a_blank_title_with_an_alert_and_keeps_the_text` |
| todos-C, kanban-RC4 | **Fixed** (the document and `theme()` apply the cookie; server.md §7): the remembered theme is not applied on load | `todos_keeps_the_dark_theme_across_a_reload`, `kanban_keeps_the_chosen_theme_across_a_reload` |
| todos-D, kanban-RC3, weather-A, notes-2, shop-D4 | **Fixed** (fallback stacks, base stylesheet; codegen.md §12): theme fonts have no generic fallback, and no base styles apply the theme to the document | `todos_draws_text_in_a_sans_serif_stack_with_base_styles`, `kanban_draws_text_in_a_sans_serif_stack`, `weather_applies_the_theme_font_and_page_colours`, `notes_applies_the_theme_font_and_colours`, `shop_draws_text_in_the_themes_sans_serif_stack` |
| todos-E | **Fixed** (scoped slots get accessors; `.autofocus`; runtime.md §5.3): a scoped-slot row is remounted when its item changes, losing the node and the focus | `todos_keeps_the_row_and_the_focus_when_a_todo_is_toggled` |
| todos-F | **Fixed** (`namespace:name` items, bare ones moved once; 36-local-storage.md): local storage keys are not namespaced | `todos_stores_its_todos_under_the_apps_namespace` |
| todos-G, weather-F | **Fixed** (`$k.description`, `/manifest.webmanifest`; 22-metadata.md): `description` and `manifest` metadata are dropped | `todos_puts_its_description_and_manifest_in_the_head`, `weather_links_and_serves_its_manifest` |
| kanban-RC1 | **Fixed** (`near`: the space around a list is the list; 41-drag-and-drop.md): a mouse drag can't drop onto an empty column | `kanban_drops_a_card_dragged_onto_an_empty_column` |
| kanban-RC2, weather-B | **Fixed** (library tokens; codegen.md §12): library tokens and base-theme assignments never reach `:root` | `kanban_puts_library_tokens_and_their_assignments_on_root`, `weather_chart_has_grid_lines_in_the_light_theme` |
| kanban-RC5 | **Fixed** (base stylesheet, `data-k-list`): no document base styles; `ReorderableList` keeps list bullets; no `color-scheme` | `kanban_has_base_styles_for_the_document_and_its_lists` |
| kanban-RC6 | **Fixed** (`.autofocus`, visible labels, charts fill their width; runtime.md §12.2): the new-card field gets no focus and no visible label; the chart is letterboxed (also weather-B) | `kanban_focuses_the_new_card_field_and_shows_its_label`, `kanban_chart_fills_its_width`, `weather_chart_fills_its_card` |
| weather-C | **Fixed** (`crate::icons`): `Icon` renders an empty `<span>` | `weather_draws_its_icons` |
| weather-D | **Fixed** (`css_value_of`): a reactive `Angle` style value has no unit | `weather_rotates_the_wind_arrow` |
| weather-E1 | **Fixed** (per-load invalidation, same-page navigations keep the page; runtime.md §7, §10): a units change re-runs every load | `weather_switching_units_fetches_only_the_forecast_again` |
| weather-E2 | **Fixed** (`$loads.ready()`): the refresh timer is recreated on every reload | `weather_creates_the_refresh_timer_once` |
| weather-E3 | **Fixed** (`@Load(keepOnReload: true)`: `reload()` keeps a marked load whose inputs are unchanged; 14-async-data.md, "Reloading"): hydrated loads don't seed the service's place cache, so the first refresh looked the place up again | `weather_refresh_after_server_rendering_uses_the_cached_place` |
| shop-D5 | **Resolved** (the behaviour is the designed one, 26-auth-and-sessions.md: a guarded page whose session ends is left for the server; the example no longer navigates after the `await`, and its doc comment and README say where sign-out lands): signing out of Admin lands on `/sign-in`, not home | `shop_admin_signs_out_to_the_sign_in_page` |
| docs-stale | **Fixed** (READMEs updated): example READMEs say the compiler doesn't exist | `example_checks.rs` `example_readmes_do_not_say_the_compiler_is_missing` |

### 4.6 Flakes

The real-browser tests run beside everything else, so a race that a quiet machine never shows comes out
under load. A flake is a bug until it is explained: either the app's (fixed, with a test of its own) or
the test's (a wait fixed to wait for a condition, never a sleep). There are no retries.

**Reproducing.** The Chrome module ten times in a row, each run beside a loop of
`cargo nextest run --workspace -E 'not package(k_cli)'`, and harder than the nextest group allows: all 44
tests at once (`cargo test -p k_cli --test it -- chrome:: --test-threads 44`) with `K_CHROME_THROTTLE=4`.
A single test is repeated by running the test binary several times at once
(`target/…/it chrome::notes_shows_an_offline … &`, sixteen copies). Diagnostics that found the causes
below, and stay: a timed-out wait names the last failed query and a hidden page, a failing test prints
its `k` process's output, and slow DevTools commands are noted.

**Found and fixed** (November 2026; the earlier two, a stream reopened under a dead session and the
notes sign-out race, are in [44-channels.md](../design/44-channels.md#decided-while-implementing)):

| Failure | Cause | Kind | Fix |
|---|---|---|---|
| `kanban_escape…`: `Input.dispatchKeyEvent: no answer within 30s` | `Tab::press` sent a `nativeVirtualKeyCode`; on macOS Chrome hands such an event to the window as a native one, and a native Escape *hides* the headless page (`visibilityState` `"hidden"`). A hidden page draws no frames, and every command that waits for one (the next key, an accessibility query) never answers. The test's wait for the closed editor had passed only because the hung query was given up after 5 s and read as "gone": Escape never closed the editor while typing, which the README and [43](../design/43-keyboard-shortcuts.md#rules) say it mustn't | test | no native key codes (the page gets the same `key`, `code`, `keyCode`); `has_no_role` counts only an answer; the test now checks what the README says: Escape in the field leaves the editor open with the text, Escape outside it closes it (`kanban_escape_closes_the_card_editor_but_not_while_typing`) |
| `weather_refresh…`, `weather_creates_the_refresh_timer_once`: `uncaught: AbortError` | the browser's `HttpClient` read the body in an `async` callback whose rejection nothing handled: a `reload()` cancelling a load after the response's head and before its body made `text()` reject | app (runtime) | `send` turns a failed body read into `offline`, which the cancelled caller ignores; `services.test.js` |
| `notes_shows_an_offline_banner…`: the banner never shows | the list's stream response, already on its way, arrived just after the `offline` event and set `Connectivity.isOnline` back to `true`; with `navigator.onLine` false nothing set it to `false` again | app (runtime) | a response or `k-online` sets `isOnline` to `navigator.onLine !== false` ([45](../design/45-web-platform.md#connectivity)); `environment.test.js`. 3 of 16 parallel runs failed before, none of 40 after |
| `notes_signs_up…`: "Enter a valid email address" after signing in again | the sign-in page is server-rendered; text typed before hydration was wiped by the form's empty fields, and a click before hydration is the browser's own submit (a reload) | app (runtime) and test | `@Form` fields keep text typed before hydration and take it (runtime.md §9; `ui-forms.test.js`), and so do plain `bind` fields since (runtime.md §5.2; `ui-hydrate.test.js`, `chrome.rs`' `text_typed_before_hydration_is_kept_in_chrome`); tests call `wait_handles_click` before the first click a freshly loaded page must handle, and `wait_handles(field, "input")` before typing into a plain `bind` field |
| `weather_shows_recent_places…`: no "Recently viewed" | the test left the city page as soon as the server-rendered heading showed, before hydration ran the effect that stores the place | test | wait until local storage holds it |
| `todos_reminds…`: no "Reminder set" | time raced from the start: the reminder could be set, shown and gone, and the button back, between two looks | test | long timers are held until the test has seen "Reminder set" |
| `weather_*`: `net::ERR_CONNECTION_REFUSED` | a port reserved by the tests was taken by a program outside them before `k run` bound it; `wait_port` connected to that program and went on, while `k` exited with `cannot listen on port` | test | `Proc::serve` waits for `k`'s own `serving on` line and restarts `k` on other ports when its port was taken |
| `Chrome's DevTools answers` | Chrome bound the port the test picked; nothing stops a program outside the tests' reservations from taking it first, and a dozen fresh profiles start slowly | test | `--remote-debugging-port=0` and `DevToolsActivePort`; 90 s |
| `textbox named "New todo"` with the field on screen; `weather_switching_units…` | each accessibility query had 5 s; on a loaded machine Chrome answered later, the query was given up and asked again behind the first, and the wait ran out. `weather_switching_units` slept 300 ms before checking that nothing else was fetched | test | queries get the full 30 s; the units test waits for the °F reading, which shows only once every load that re-ran has answered (so an extra request would already have been made) |
| `notes_stops_promptly…` | slept 500 ms for the stream to open and 300 ms for the page's errors after stopping the server | test | waits for the stream's response (`Network.responseReceived`); `allow_errors_from_now_on` for a page whose server is gone |
| `notes_shows_an_offline_banner…`: `the page logged errors: log: Failed to load resource: net::ERR_INTERNET_DISCONNECTED` | `allow_errors()` accepted the errors logged so far, but Chrome can report a request that failed offline after the network is back, past that count | test | `Tab::allow_errors_containing(since, "ERR_INTERNET_DISCONNECTED")`: from the error count when the network went, that error only, whenever it is reported; any other error still fails the test |
| `collections_share_one_database_across_two_builds_in_two_tabs_in_chrome`: `"1 entries"` never shows (the newer tab stays at `0 entries`) | probes on `indexedDB.open` showed the newer tab's `open(2)` waiting about 23 s for its `upgradeneeded`: the older tab, in the background, handled `versionchange` and closed its connection only then, because Chrome runs a background renderer at a low process priority and a loaded machine starved it; the app gives way correctly (no deadlock, no second upgrade) | test | the test's Chrome keeps background tabs at full priority (`BACKGROUND_TABS_RUN`: `--disable-renderer-backgrounding` and the timer and occlusion flags, as `FAKE_CALL` already did for calls); twelve consecutive runs beside twenty busy processes on 14 cores were clean (about 1.5 s each), where 2 of 12 failed before |

**Result:** ten consecutive runs of all 44 tests at once under a 4× CPU throttle beside the load loop
were clean, and full `cargo nextest run --workspace` runs with the `chrome` group at 4, 8 and 12 were
clean too (4: 24–25 s; 8: 26–31 s; 12: 26–29 s), so the group stays at 4. Beyond about 44 Chromes at
once (two such runs side by side, or 80 kanban tabs) Chrome stops answering `DOM.getDocument` within
30 s while `Runtime.evaluate` still answers; that is past anything the configuration runs.

---

## 5. Property tests

Property tests use [proptest](https://docs.rs/proptest). They state invariants that must hold for *every*
input, and run on stable as part of `cargo test`.

### 5.1 The syntax oracle

All syntax invariants are checked by one module, `crates/k_syntax/tests/support/oracle.rs`, which the
property tests *and* the fuzz targets (§6) include, so a property is never checked one way on stable and
another way under fuzzing. `oracle::check_all(text)` lexes and parses `text` and asserts:

| Check | Invariant |
|---|---|
| `check_lexed` | tokens are non-empty token kinds, contiguous from offset 0, on char boundaries, and concatenate to the input; fixed-text kinds (keywords, punctuation) have their spelling |
| `check_diagnostics` | primary spans, labels and fix edits lie in the file and on char boundaries; codes are registered; messages are one line; edits of one fix do not overlap; diagnostics are sorted |
| `check_tree` | the root is a `SourceFile` covering the input; the tree text equals the input exactly; the tree's tokens are the lexer's tokens (same ranges and texts, same kinds except contextual keywords the parser remapped from `Ident`, grammar §6) |
| `check_lexer_diagnostics_are_reported` | every lexer diagnostic is in the parse's diagnostics |
| `check_relex` | re-lexing the concatenated texts of the tree's tokens reproduces the token stream, offsets and diagnostics |
| `check_dumps` | the `.tokens` dump reads back to the same token lines; the `.ast` dump reads back to the same S-expression |
| `check_error_bound` | at most `2 × tokens + 2` diagnostics for `tokens` non-trivia tokens: one mistake must not cascade |
| `check_deterministic` | lexing and parsing again give identical kinds, offsets, green trees and diagnostics |

`oracle::check_fast` is the cheap subset (no dumps, no second parse) used by the throughput-oriented
`parse` fuzz target. `oracle::check_no_duplicates` (no diagnostic with the same code and span reported
twice) is part of `check_all`; the inputs that once violated it are `FORMER_DUPLICATES` in
`properties.rs` and the `duplicate-diagnostic-*` cases in `tests/corpus/parser/regressions/`.

### 5.2 Inputs

`crates/k_syntax/tests/properties.rs` runs the oracle over:

- **arbitrary text**: any Unicode string, and any byte string decoded lossily (invalid UTF-8 becomes
  U+FFFD, a non-ASCII error character);
- **K-shaped text**: fragments of real and broken K syntax concatenated (`FRAGMENTS`), which reaches the
  string, interpolation and style lexer modes. When a construct is added, add its fragments;
- **generated programs**: the structure-aware generator `crates/k_syntax/tests/support/kgen.rs` (§5.3)
  fed with proptest bytes; programs generated in *valid* mode must parse with **no diagnostics**;
- **derived inputs**: every prefix of a generated program (a cut at any char boundary models typing), and
  broken K-shaped text spliced into the middle of a generated program;
- the same text under a different `FileId`, which must change nothing but the spans' file;
- a fixed list of edge cases (empty input, lone BOM, CR, unterminated strings, comments and blocks, ...).

`PROPTEST_CASES=N` raises the number of cases per property (default 384). Failing inputs are shrunk,
printed, and persisted in `crates/k_syntax/tests/properties.proptest-regressions` (check that file in);
each finding should also become a corpus regression case (§7).

### 5.3 The structure-aware generator

`kgen::generate(&mut arbitrary::Unstructured)` walks the grammar of [grammar.md](grammar.md): imports,
every declaration kind with its clauses, members, statements, expressions (precedence, postfix chains,
trailing lambdas, `try`/`await`, `bind`, `when`), patterns, types, views with modifiers, style blocks
(selectors, declarations, interpolations, `@media`) and tests, with comments, blank lines and CRLF in
between. A node budget and a depth limit keep programs small (median about 2.5 KiB) and below the
parser's nesting limit.

The first choice picks the mode: *valid* (two thirds) writes only well-formed syntax, so any diagnostic is
a parser bug or a generator bug; *noisy* drops, doubles or prefixes tokens with broken fragments to reach
error recovery from realistic surroundings. When the bytes run out every choice takes its first option,
so the generator always terminates and empty input yields the smallest valid program.

When a valid-mode program reports a diagnostic, decide from the grammar which side is wrong. If the
generator wrote invalid syntax, fix the generator; if the syntax is valid, it is a parser bug (§7).

### 5.4 Other suites

`crates/k_syntax/tests/invariants.rs` is the original, minimal statement of the four core invariants
(no panic, lossless tokens, lossless tree, spans in bounds). `crates/k_syntax/tests/robustness.rs`
complements the properties without proptest: a deterministic random stress run over K-shaped fragments
(every invariant above plus message style; in debug builds the parser's fuel assertion also catches loops
that do not consume input), every recursive construct nested past the parser's limit on a bounded stack
(K0134), very deep trees dropped on a 1 MiB stack, and quick fixes (applying the machine-applicable ones
yields code that parses cleanly; the others are marked `maybe-incorrect`). `K_STRESS_CASES=N` raises the
number of random inputs (default 3000).

`crates/k_syntax/tests/adversarial.rs` holds regressions from adversarial testing that the corpus cannot
express: recursion that must be bounded by the nesting limit (function-type chains, `if`/`match` nested
in conditions) and dumps of very deep trees. Run it with nextest (a process per test): a stack overflow
aborts the whole process, which under `cargo test` is all of `k_syntax::it`.

Later stages add their own property files: code generation and execution are §5.6; the formatter
(deferred) will state that formatting is idempotent.

### 5.5 Semantic properties

`crates/k_sema/tests/properties.rs` states the invariants of semantic analysis over whole projects. Like
the syntax properties, every check lives in one module, `crates/k_sema/tests/support/soracle.rs`, that the
`check*` fuzz targets (§6) include too. A project is a list of `(project path, text)` pairs; diagnostics are
compared in *normal form* (`soracle::Norm`), with files named by path rather than by `FileId`, because file
ids depend on the order files were added.

| Check | Invariant |
|---|---|
| `check_diagnostics` | codes registered; messages one non-empty line; primary spans, labels and fix edits in a known file, in bounds and on char boundaries; no fix edits a standard-library file; the edits of one fix are sorted and disjoint; diagnostics sorted by span and code; `file_diagnostics(f)` is exactly the slice of `diagnostics()` in `f` |
| `check_deterministic` | a fresh analysis of the same inputs, and one with `Options::with_threads(1)`, report exactly the same list |
| `check_order_independent` | adding the files in another order reports the same diagnostics up to file ids |
| `check_incremental` | after each edit of a sequence (splice into a file, replace, remove or add a file, or set unchanged text), the re-check by the same host equals a fresh check of the edited project, and the snapshot taken before the edit is cancelled |
| `check_fixes_add_no_errors` | applying every machine-applicable fix (identical fixes once; overlapping ones skipped, as `rustfix` does) never increases the number of errors of any code in any file |
| `check_queries` | `type_at`, `resolve_at` and `side_of` answer or decline on every token range without panicking |

The inputs:

- **arbitrary text** and lossily decoded bytes, analysed as the only file of a project at a module path, a
  test file, `App.k` and a stray path;
- **syntax-generated projects**: one to three programs of the grammar generator (§5.3) in two modules,
  mostly ill-typed, which reach the checker's recovery paths;
- **well-typed projects** from the typed generator `crates/k_sema/tests/support/tgen.rs`, which must check
  with **zero errors**;
- **mistaken projects**: a well-typed project with one to four fixable mistakes injected
  (`tgen::inject_fixable_mistakes`: duplicate, unused, misspelt or missing imports, `&&`, `||`, `!`, a
  semicolon), for the fix property;
- **the `sema` corpus**: every case is loaded as the corpus suite does, and its machine-applicable fixes
  are applied (`machine_fixes_in_the_corpus_add_no_errors`);
- a fixed list of edge-case projects (no files, empty or broken `k.toml`, self-referential types, …).

**The typed generator** keeps a type environment and generates every expression *for* a type. A project
has a `model` module of public types, one per file named like the file, and a `logic` module of private
enums, structs, functions and top-level constants over one to four files, which import exactly the
`model` types they name. It covers `Int`, `Double`, `Bool`, `String`, `T?`, `List<T>`, enums with and
without payloads, structs with `let`/`var` fields, defaults and methods; literals, memberwise
initializers, qualified and implicit enum cases, arithmetic, comparisons, `and`/`or`/`not`, concatenation,
interpolation, `??`, `.count`, fields, calls, methods and `if` expressions; and `let`/`var`, assignment,
`if`, `while`, `for`, `if let`, `guard let`, exhaustive and `default` `match` over enums and optionals,
void calls and `return`. It avoids what is ill-typed by design (`==` on enums and structs, implicit
members and `nil` without an expected type, shadowing). `typed_generator_covers_its_constructs` fails if a
construct stops being generated. When a well-typed project reports an error, decide from
[08-types.md](../design/08-types.md) and [sema.md](sema.md) which side is wrong: fix `tgen.rs` if it
wrote an ill-typed program; otherwise it is a checker finding (below).

**Known findings.** A checker bug found by these properties is reported, pinned and *not* worked around
silently:

- a wrong error on a well-typed program becomes a pending corpus case under
  `tests/corpus/sema/regressions/` (§7), and the typed generator steers around exactly that shape, citing
  the case in `tgen::KNOWN_FINDINGS`;
- a machine-applicable fix that adds errors is listed in `KNOWN_BAD_FIXES` in `properties.rs`. Like a
  `.pending` file, a listed corpus case must keep failing; once its fix is repaired the test fails and asks
  for the entry to be removed.

When a finding is fixed, delete the `.pending` file, the `KNOWN_*` entry and the workaround together.

Default case counts are lower than the syntax properties' (256 for arbitrary text, 128 for project
properties) because each case runs several whole-project analyses; the whole file takes about 15 s in a
debug build. `PROPTEST_CASES=N` sets every property's count.


### 5.6 Stage 3 properties: code generation, execution, the server and SQL

Stage 3 is checked **differentially**: generated programs are compiled by `k_codegen` and run in the
embedded V8 (`k_server::Engine`), and what they do is compared with an independent reference, with
another build of the same program, or with the other renderer. As in §5.1 and §5.5, every check lives in
a support module that the property tests and the fuzz targets (§6) share, under
`crates/k_server/tests/support/`:

| Module | Role |
|---|---|
| `coracle.rs` | the codegen oracle: `check_codegen` (every profile generates; a fresh analysis generates byte-identical output; `check_client_bundle` finds no server code), `run` (executes a script and classifies the outcome), `check_terminating_program` |
| `exprgen.rs` | closed expressions over `Int`, `Double`, `Bool`, `String` with a reference evaluator in Rust |
| `codablegen.rs` | `Codable` declarations and values with the reference JSON encoding, JSON mutations and a reference decoder |
| `viewgen.rs` | components with generated views and two sets of props |
| `horacle.rs` | the HTTP oracle over the app `crates/k_server/tests/projects/fuzz-wire` |

`crates/k_server/tests/differential.rs` states the properties; `crates/k_sema/tests/support/tgen.rs` (the
stage 2 typed generator, §5.5) gained a **runnable mode**, `tgen::generate_runnable`, for them.

**Outcomes.** `coracle::run` runs `run()` of a script's entry module (the `run` corpus' protocol, §3.10)
in a fresh isolate. `returns`, `throws` and `panics` are K outcomes. Because `run()` reports any
JavaScript exception as `panics <message>`, a panic is re-run by calling the entry function directly to
read the exception's class: only a `KPanic` is a K outcome. A `TypeError`, a `ReferenceError`, a module
that fails to load (a syntax error in generated code) or anything else is `Unexpected`, which is always a
code generation or runtime bug. A time-out is a failure for the generators below, which only write
programs that terminate.

| Property | Input | Invariant |
|---|---|---|
| `typed_programs_generate_deterministically_and_run` | `tgen::generate_runnable` | `check_codegen`; the development build runs to a K outcome; the release build (renamed locals, compact printing) logs the same lines and ends the same way, unless the development build panicked with an integer overflow (release builds don't check `Int` `+ - *`, [codegen.md §5](codegen.md)) |
| `closed_expressions_agree_with_the_reference` | `exprgen::generate` | as above, and `main` returns exactly the reference values, written **inline** (all literals, which the code generator may fold) and **opaque** (every literal behind an identity function, which it can't); a panic is the reference's first panic in evaluation order |
| `codable_values_round_trip_and_decode_like_the_reference` | `codablegen::generate` | `Json.encode` writes the reference JSON; decoding it gives an equal value (`==`) that encodes the same; every mutated text decodes to the reference decoder's value, `malformed` or `mismatch` |
| `server_html_hydrates_to_the_client_render` | `viewgen::generate` | the server HTML parses back to itself (escaping is complete); the client render is the same HTML; hydration leaves the same HTML and **keeps every server node** (it may only add the empty text node an empty dynamic text has); after a props change the hydrated DOM equals the client-rendered DOM and a fresh server render |
| `the_server_answers_the_fuzz_seeds`, `rpc_bodies_never_fail_the_server` | the seeds of the server fuzz targets; seeds with a byte changed; arbitrary bytes | `horacle`: a raw request gets no answer or a well-formed HTTP/1.1 answer that isn't a `500`, and the server still serves `/home` and an rpc; an rpc body gets a `200` JSON message that is a **fixed point** (posting it back answers the same) or a `400 invalid_argument` |
| `pinned_findings_still_reproduce` | the minimized findings the `run` corpus can't hold | each still fails as recorded (below); none is open today |
| `fixed_findings_stay_fixed` | the reproductions of every finding that was pinned there, plus the cases found while fixing them | each runs and agrees with the reference |

**The runnable typed generator.** `tgen::generate_runnable` writes the project `tgen::generate` writes,
plus `logic.main` (`tgen::RUNNABLE_ENTRY`), which calls every function once with generated arguments and
logs each primitive result. Three changes make every execution finite without changing what is
well-typed: calls are acyclic (`f<i>` only calls `f<j>`, `j < i`; methods call nothing), and every
`while` loop also counts to three in a private counter. The non-runnable mode, which the stage 2
properties use, is unchanged.

**The reference semantics** (`exprgen`): `Int` is a safe integer (±(2^53 − 1)); `+ - *` and unary `-`
panic with `integer overflow: … is outside the Int range` beyond it; `/` truncates toward zero and `%`
has the dividend's sign, and both panic with `division by zero`; `Double` is IEEE 754 (`%` is `fmod`);
`and`, `or` and `if` evaluate only what they need. Strings compare by UTF-16 code units; `count`,
`prefix` and interpolation follow the `run` corpus (`tests/corpus/run/{ints,doubles,strings}`). Where the
format is unspecified (a non-finite `Double` result, a `Double` interpolated outside `[1e-6, 1e15)` or
`-0.0`), the generator writes a literal instead, so the reference never depends on it. The string
alphabet has quotes, backslashes, braces, markup, two-byte, three-byte and astral characters, but nothing
that combines into a larger grapheme cluster.

**The reference wire format** (`codablegen`, [28-rpc-wire-format.md](../design/28-rpc-wire-format.md),
[codegen.md §5.1](codegen.md)): fields by K name; payload-free cases as their name; payload cases as
`{"case": name, label: …}`; `nil` as `null`; `Map<String, T>` as an object; decoding ignores unknown keys,
reads a missing or `null` optional as `nil`, and accepts as an `Int` any JSON number with a safe-integer
value (`1.0` and `1e2` included: JavaScript can't tell them from `1` and `100`); a number beyond the
`Double` range is JSON (an `Int` `mismatch`, a `Double` infinity that encodes as `null`), never `malformed`
([codegen.md §5.1](codegen.md#51-json-codable)). Map keys include names
special to JavaScript objects (`constructor`, `toString`, `hasOwnProperty`). The mutations replace a node
by every JSON shape, drop or add keys, write an integer as `1.0` or `1e0`, truncate, pad or append to the
text, and repeat a key (the last one wins, as in `JSON.parse`). A payload-free case written as
`{"case": "c0"}`, and a payload case whose fields are all optional written as its bare name `"c0"`, are
shapes the wire format leaves open (the decoder accepts both today): either a value or `mismatch` is
accepted, never `malformed` or an exception.

**The view generator** (`viewgen`) nests `box`, `section`, `article`, `paragraph`, `text`, `emphasis`,
`heading`, `list`/`item` and `button`, with static texts that need escaping (`<script>`, `-->`,
`&amp;`, quotes), dynamic texts and interpolations of the props, `if`/`else`, keyed `for` loops and
dynamic `.label` attributes. The driver imports the client component and `hydrate`, `signal` and `batch`
from the **client runtime chunk** (`runtime.<hash>.js`): the client chunks import that module instance,
not `k:runtime`, and hydration state lives in it. Two spellings of the same markup are normalized before
comparing: the fake DOM serializes an empty attribute as `name` (HTML: `name=""`) and doesn't escape `'`
in attribute values (the server writes `&#39;`).

**SQL.** `crates/k_db/tests/sql_properties.rs` generates query IR over tables whose names and columns are
hostile identifiers (`order`, `a"b`, `Robert'); DROP TABLE t;--`, `$1`, …) and values (quotes, `%`, `_`,
`\`, `--`, `' OR '1'='1`):

- `rendered_sql_interpolates_nothing`: in both dialects and for every statement kind, once quoted
  identifiers (each unquoted back to one of the statement's names) and numbered parameters (each at most
  `Sql::params`, `?n` on SQLite and `$n` on Postgres) are taken out, the text is only keywords,
  punctuation and the renderer's own constants (`1 = 1`, `LIMIT -1`, `ESCAPE '\'`, a source `LIMIT 50`);
- `sqlite_agrees_with_the_reference`: rows inserted with rendered `INSERT`s read back exactly, and
  rendered `SELECT` (with `LIMIT`/`OFFSET` parameters), `ORDER BY` a nullable text column (`NULL` first
  ascending, last descending, byte order), `COUNT`, `UPDATE … SET c = (c + ?)` and `DELETE` return or
  change exactly the rows of a three-valued-logic reference (SQL `NULL`, `x IN ()` false even for `NULL`,
  case-sensitive `LIKE` with its escapes).

`crates/k_codegen/tests/persistence_properties.rs` closes the loop from K: generated repositories filter,
search (`contains`, `startsWith`, `endsWith`), sort and limit with hostile constants and captured
parameters, and no statement of the database manifest holds a string literal, a number other than a
source `limit`, or any constant of the program; the server JavaScript names statements by number and
holds no SQL; no client file holds the repository.

**Running and minimizing.** The defaults are 64 cases per V8 property and 128 for rpc bodies (about 10 s
in a debug build; each case compiles a project and starts isolates); `PROPTEST_CASES=N` raises them all
(1,000 each take about 90 s on 6 threads). `sql_properties` runs 256 cases of each property in well under
a second, and `persistence_properties` 48 analyses in about a second. Two ignored tests replay a finding by hand while it is minimized:

```sh
K_REPLAY_DIR=path/to/project K_REPLAY_ENTRY=calc.main K_DIFF_DUMP=1 \
  cargo test -p k_server --test it -- --ignored differential::replay_program --nocapture
K_REPLAY_VIEW=path/to/Generated.k K_REPLAY_PROPS='[{"title":"a","count":0,"isOn":true,"items":[]},{…}]' \
  K_DIFF_DUMP=1 cargo test -p k_server --test it -- --ignored differential::replay_view
```

`K_DIFF_DUMP=1` prints the release build's modules (`replay_program`) or every module of the view case.

**Known findings.** A bug these properties find is minimized, pinned and steered around, never worked
around silently (as in §5.5):

- a finding the `run` suite can express (a development build, server side) becomes a pending case in
  `tests/corpus/run/regressions/fuzz/` with a row in its `README.md`;
- a finding that needs a release build or the client DOM is an entry of `pinned_findings_still_reproduce`
  in `differential.rs`, with the symptom its failure must mention; the test fails once the finding is
  fixed (or fails differently);
- the generator that found it steers around exactly that shape, citing it in its `KNOWN_FINDINGS`
  (`tgen.rs`, `exprgen.rs`, `codablegen.rs`, `viewgen.rs`). When the finding is fixed, the pending file or
  pin, the `KNOWN_FINDINGS` entry and the workaround go together; a pin's reproduction moves to
  `fixed_findings_stay_fixed`, and a fixed corpus case stays as a regular case.

The findings of the first runs:

| Finding | Pinned in | Workaround |
|---|---|---|
| the release printer writes `a - (-b)`, `a - (-3)` and `x -= (-x)` as `a--b`: a syntax error | `pinned_findings_still_reproduce` | `exprgen` rewrites `a - (-b)` to the equal `a + b`; runnable `tgen` writes negation as `(0 - a)` |
| a `match` over an enum with a `default` arm gets two `default:` clauses: the module fails to load | `run/regressions/fuzz/match-default-over-an-enum.k` | runnable `tgen` writes every enum `match` exhaustively |
| `Json.encode` drops a `Map` entry whose key is `__proto__` | `run/regressions/fuzz/map-key-proto-is-encoded.k` | no generated map key is `__proto__` |
| a JSON array decodes as an empty `Map` instead of `mismatch` | `run/regressions/fuzz/map-decodes-only-objects.k` | no object or `null` node is mutated into `[]` |
| a static text next to another text (`paragraph { "a" title }`, `"a" "b" text { … }`) or an empty static text shares one text node with it in the parsed template, but the element paths count them apart: the client render throws a `TypeError` | `pinned_findings_still_reproduce` | static texts are never empty; a text next to a static text is wrapped in `text { }` |
| hydration re-creates, instead of claiming, a text or a nested block at the top of an `if` branch, although the HTML matches | `pinned_findings_still_reproduce` | each `if` branch is one `box` |
| hydrating a view whose root is a block (`if` at the top) adds a second pair of block markers | `pinned_findings_still_reproduce` | the generated view's root is one `box` |

None of these was fixed by the commit that pinned it. **All are fixed now** (stage 3, implementation
round 2), every `KNOWN_FINDINGS` list is empty and the generators write these shapes again:

| Finding | Fix |
|---|---|
| `a--b` in release builds | the compact printer puts a space between a binary `-`/`+` and a right operand that starts with the same sign (`js.rs`) |
| two `default:` clauses | a `default` arm (no patterns) is the `switch`'s `default:` (`lower.rs`) |
| `__proto__` map key, arrays as maps | the map codec encodes text-keyed maps into a prototype-less object and decodes each shape only from itself: objects for text keys, `[key, value]` pairs otherwise (`std.js`) |
| adjacent and empty static texts | adjacent texts with a static one merge into one text node on both sides, empty static texts are dropped (`view.rs` `merge_texts`) |
| texts and blocks at the top of a branch, block roots | `textNode`, `separator` and `markers` claim the server's text, `<!---->` and marker pair while hydrating (`hydrate.js`) |

---

## 6. Fuzzing

Fuzz targets use [cargo-fuzz](https://rust-fuzz.github.io/book/cargo-fuzz.html) (libFuzzer with
AddressSanitizer). They need a nightly toolchain, so they live outside the stable workspace in `fuzz/`
(`exclude = ["fuzz"]` in the root `Cargo.toml`).

### 6.1 Setup

```sh
rustup toolchain install nightly --profile minimal
cargo install cargo-fuzz
fuzz/seed-corpus.sh            # seed fuzz/corpus/<target>/ (see below)
fuzz/seed-corpus.sh --fresh    # the same, after setting the semantic targets' grown corpora aside
```

`fuzz/corpus/`, `fuzz/corpus-old/` and `fuzz/artifacts/` are git-ignored working state. Re-run
`seed-corpus.sh` after adding corpus cases; it keeps what the fuzzer has already found.

**Seed sets.** libFuzzer runs every corpus input once before it mutates anything, so a target's seed set
must load in a small part of a short run. The seeds are chosen per target:

| Targets | Seeds | Input cap |
|---|---|---|
| `lex`, `parse`, `parse_roundtrip`, `structured` | every `tests/corpus/**/*.k` file | `-max_len=8192` |
| `check` | about 140 files: the median-sized single-file case of each `sema` directory two levels down, and every `regressions` case of `sema`, `parser` and `lexer`, each at most 2 KiB | 2 KiB (`CHECK_MAX_INPUT`) |
| `check_structured`, `check_typed` | 16 random inputs of 64 to 1,024 bytes (their bytes drive a generator, so K source is no better a seed than noise) | 1 KiB (`GENERATOR_MAX_INPUT`) |
| `codegen`, `codegen_run` | 16 random inputs of 256 to 4,096 bytes | `-max_len=4096` |
| `http_request`, `rpc_json` | `fuzz/seeds/<target>/` | `-max_len=2048` |

The two caps are constants in `fuzz/src/lib.rs`: the targets return at once for a longer input, so a
corpus entry over the cap costs nothing whatever `-max_len` says; pass the same value as `-max_len` so
mutations stay under it. Before stage 4 the semantic targets were seeded with the whole golden corpus
(2,600 files, plus everything they had grown), and loading it took `check`, `check_structured` and
`check_typed` 165 to 369 seconds before the first mutation. With these seed sets each loads in 1 to 2 seconds
(measured on the machine of §6.3), so a 30-second run fuzzes for about 28 seconds: `check` runs about 5,600
inputs and grows its corpus to about 1,100 entries, `check_structured` and `check_typed` about 400 each.

A corpus keeps growing across runs and slows its own loading again. When a semantic target's `INITED`
line takes more than a few seconds, run `fuzz/seed-corpus.sh --fresh`: it moves the `check`,
`check_structured` and `check_typed` corpora to `fuzz/corpus-old/<target>.<seconds>` (nothing is deleted)
and seeds them anew; merge back what matters with `cargo +nightly fuzz cmin <target>`.

### 6.2 Targets

| Target | Input | Asserts |
|---|---|---|
| `lex` | arbitrary bytes, decoded lossily | `check_lexed` and valid lexer diagnostics |
| `parse` | arbitrary UTF-8 (other inputs skipped) | `check_fast`: no panic, `SourceFile` root, valid diagnostics, bounded error count |
| `parse_roundtrip` | arbitrary UTF-8 | `check_all`: the tree text equals the input exactly, token identity, re-lexing, dumps, determinism |
| `structured` | bytes for `kgen::generate` | `check_all`, and no diagnostics at all for valid-mode programs |
| `check` | first byte picks the path (module file, test file, `App.k`, stray file); the rest is UTF-8 | semantic analysis of a one-file project: no panic, `check_diagnostics`, `check_queries` |
| `check_structured` | bytes for one to three `kgen` programs in two modules, then edits | `check_diagnostics`, `check_deterministic`, `check_order_independent`, `check_incremental` |
| `check_typed` | bytes for `tgen::generate`, then edits and mistakes | zero errors, `check_order_independent`, `check_incremental`, `check_fixes_add_no_errors` |
| `codegen` | first byte picks `tgen::generate` or `generate_runnable`, the rest drives it | `coracle::check_codegen`: every profile generates, deterministically, with no server code in the client bundle (no V8) |
| `codegen_run` | first byte picks `tgen::generate_runnable`, `exprgen`, `codablegen` or `viewgen`, the rest drives it | the property of §5.6 for that generator, run in V8 (one isolate at a time) |
| `rpc_json` | first byte picks an echo rpc of the fuzz app, the rest is the request body | `horacle::check_rpc_body`: a `200` fixed point or `400 invalid_argument`, never a `500` |
| `http_request` | raw bytes written to a connection of the fuzz app's server | `horacle::check_raw_request`: well-formed HTTP/1.1 answers, never a `500`, still serving afterwards |
| `lsp_edits` | first byte picks an example document, the rest drives incremental edits and requests (`crates/k_lsp/tests/support/driver.rs`) | one well-formed response per request, only expected error codes, the server keeps serving (one in-process server per process) |

All targets share `crates/k_syntax/tests/support/{oracle,kgen}.rs`,
`crates/k_sema/tests/support/{soracle,tgen}.rs` and
`crates/k_server/tests/support/{coracle,exprgen,codablegen,viewgen,horacle}.rs` and
`crates/k_lsp/tests/support/driver.rs` through `fuzz/src/lib.rs`.
The server targets start the fuzz app (`crates/k_server/tests/projects/fuzz-wire`: a page, two endpoint
pages, one echo rpc per message shape) once per process with one worker; their seeds are
`fuzz/seeds/<target>/` (the same requests as `RAW_SEEDS` and `RPC_SEEDS` in `horacle.rs`), copied into the
corpora by `seed-corpus.sh`, and `fuzz/http.dict` holds HTTP tokens for `http_request`. The V8 targets link
the prebuilt V8 of `deno_core`; set `RUSTY_V8_ARCHIVE=target/release/gn_out/obj/librusty_v8.a` (the archive
the stable build downloaded) so the fuzz build doesn't download it again. Debug assertions are on in fuzz
builds, so the parser's fuel assertion (a loop that does not consume input) is a crash too.

### 6.3 Running

From the repository root:

```sh
cargo +nightly fuzz list
cargo +nightly fuzz run parse_roundtrip -- -max_total_time=600 -dict=fuzz/k.dict
cargo +nightly fuzz run structured -- -max_total_time=600 -max_len=8192
cargo +nightly fuzz run lex -- -jobs=8 -workers=8 -max_total_time=3600 -dict=fuzz/k.dict
cargo +nightly fuzz run check -- -max_total_time=600 -max_len=2048 -dict=fuzz/k.dict -timeout=20
cargo +nightly fuzz run check_structured -- -max_total_time=600 -max_len=1024 -timeout=20
cargo +nightly fuzz run check_typed -- -max_total_time=600 -max_len=1024 -timeout=20
export RUSTY_V8_ARCHIVE=$PWD/target/release/gn_out/obj/librusty_v8.a
cargo +nightly fuzz run codegen -- -max_total_time=600 -max_len=4096 -timeout=30
cargo +nightly fuzz run codegen_run -- -max_total_time=600 -max_len=4096 -timeout=60 -rss_limit_mb=6144
cargo +nightly fuzz run rpc_json -- -max_total_time=600 -max_len=2048 -timeout=30
cargo +nightly fuzz run http_request -- -max_total_time=600 -max_len=2048 -timeout=30 -dict=fuzz/http.dict
```

Useful libFuzzer flags: `-max_total_time=S` (stop after S seconds), `-max_len=N` (input size cap; 8192 is
plenty for syntax), `-timeout=S` (report an input that runs longer than S seconds; a hang is a bug, since
parsing is linear), `-rss_limit_mb=N`, `-dict=fuzz/k.dict` (keywords, operators and lexer-mode switches,
which helps the byte-level targets form tokens), `-jobs=N -workers=N` (parallel runs),
`-print_final_stats=1`.

Every syntax target must run clean for at least 60 seconds before a syntax change is merged, and every
`check*` target before a checker change is merged; run them for
longer (10 minutes or more each) after changes to the lexer's modes, recovery or nesting limits. Typical
throughput on an M-series laptop with sanitizers: `lex` about 4,800 inputs/s, `parse` about 2,600,
`parse_roundtrip` about 500 and `structured` about 200. The semantic targets are much slower because
every input is one or more whole-project analyses, standard library included: `check` about 180
inputs/s, `check_structured` and `check_typed` about 11–13 (several analyses per input). Their seed sets
load in 1–2 seconds (§6.1), so even a 30-second run fuzzes; run them with `-fork=N` (or `-jobs`) and for
at least 10 minutes when their oracle or generator changes.

The stage 3 targets must each run clean for at least 60 seconds before a change to code generation, the
runtimes or the server is merged. Measured throughput (M-series laptop, sanitizers on): `codegen` about
18 inputs/s (three profiles, two analyses each), `codegen_run` about 10 (an analysis, three generations
and one to three isolates per input), `rpc_json` about 2,000–2,500 (one or two rpcs on a keep-alive
connection), `http_request` about 350. `http_request` opens a connection per input, and on loopback both
ends of a closed connection stay in `TIME_WAIT` for 30 s (macOS), so `horacle` paces new connections to 400
a second to stay below the 16,384 ephemeral ports; well-formed requests (health checks, rpcs) share one
keep-alive connection. `codegen_run` grows by about 1 MB of RSS per input under AddressSanitizer (V8 heaps
in quarantine), hence `-rss_limit_mb=6144`; for runs longer than about 10 minutes, use `-fork=N`, which
restarts workers. `-len_control=0` with the random seeds of `seed-corpus.sh` makes the generator targets
start from programs of every size instead of growing from tiny inputs.

### 6.4 When a target crashes

libFuzzer stops and writes the input to `fuzz/artifacts/<target>/crash-<hash>` (or `timeout-`, `oom-`).

1. Reproduce: `cargo +nightly fuzz run <target> fuzz/artifacts/<target>/crash-<hash>`.
2. Minimize: `cargo +nightly fuzz tmin <target> fuzz/artifacts/<target>/crash-<hash>`. For
   `structured`, the artifact is generator bytes; the panic message prints the generated K source, which
   is what you minimize (by hand or with the `parse_roundtrip` target).
3. Triage. For `structured`, a valid-mode diagnostic may be a generator bug: check the source against
   [grammar.md](grammar.md) and fix `kgen.rs` if the generator wrote invalid syntax.
4. Record the minimized K source as a corpus regression case **before** fixing the bug (§7), under
   `tests/corpus/<suite>/regressions/fuzz/` (for example `tests/corpus/parser/regressions/fuzz/`).
   Every top-level directory of `tests/corpus` is a suite, so fuzz regressions live inside the suite they
   exercise, not in a separate top-level directory. Write the expectation files by hand from the grammar
   and add a row to the directory's `README.md` naming the target and the symptom.
5. Fix the bug; the corpus case and the fuzz artifact must then pass.

For the `check*` targets, step 3 and 4 differ:

- **Triage.** For `check_typed`, an error on the generated project is triaged like a failing
  well-typed property (§5.5): a generator bug is fixed in `tgen.rs`; anything else is a checker finding.
  A failed fix, order or incremental property is always a checker finding (or an oracle bug in
  `soracle.rs`). The panic message prints every file of the project, which is what you minimize.
- **Pin.** Record the minimized project as a case under `tests/corpus/sema/regressions/fuzz/` with its
  expected `.diag` written by hand, a `.pending` file naming what is missing, and a row in that
  directory's `README.md` (target and symptom). A pending case passes while the checker is wrong and fails
  once it is fixed, so the suite stays green and the finding cannot be forgotten. If the finding keeps
  the target from exploring further, the generator or mistake injector steers around exactly that shape,
  citing the case in `tgen::KNOWN_FINDINGS`.
- **Panics.** A case that panics the checker fails even when pending (the corpus harness reports a panic
  as a failure), so a panic is pinned in the `README.md` row with its minimized source and turned into a
  case by the commit that fixes it.

For the stage 3 targets (`codegen*`, `rpc_json`, `http_request`) the panic message prints the
generated project (and props), or the request; minimize it by hand with the `replay_program` and
`replay_view` tools of `differential.rs` (§5.6), then pin it as §5.6 says (a pending `run` case under
`tests/corpus/run/regressions/fuzz/`, or an entry of `pinned_findings_still_reproduce`) and steer the
generator around it (`KNOWN_FINDINGS`). A failure that turns out to be the oracle's (an HTTP/1.0 answer, a
transport error of the fuzzer's own socket table) is fixed in the oracle.

The formatter (deferred) will add its targets on the same pattern: an oracle module shared with a stable
property test, a byte-level target and a structure-aware one.

---

## 7. Regression tests

Every fixed bug gets a test that fails without the fix:

- For a compiler bug, add a corpus case under `tests/corpus/<suite>/regressions/` (and a row in that
  directory's `README.md`), named
  `<short-description>.k` (for example `regressions/crlf-inside-interpolation.k`). Put a `#` comment at the
  top of the expectation file (`.tokens` or `.diag`), or a `;` comment in `.ast`, explaining the bug.
- For a fuzz or proptest finding, the minimized input becomes the case's source; fuzz findings go in
  `tests/corpus/<suite>/regressions/fuzz/` (§6.4).
- A finding that is reported before it is fixed (a checker finding of the semantic properties or fuzz
  targets, §5.5 and §6.4) is pinned the same way with a `.pending` file, so it fails once fixed. The
  `KNOWN_*` lists that steer the generators around it are emptied by the same commit that deletes the
  `.pending` file.
- For runtime or server bugs, add the test next to the code it fixes, with a comment naming the bug.
- Findings of adversarial testing that need a generated app (rpc decoding and validation, client
  routing and hydration, keyed rows, XSS sinks, sessions, slow clients) are in
  `crates/k_server/tests/regressions/main.rs`, one test per finding, serving the K projects in
  `crates/k_server/tests/regressions/projects/`; runtime-only findings are in
  `crates/k_runtime/js/tests/regressions.test.js` (`test.pending`) and language-semantics findings in
  `tests/corpus/run/regressions/`. An open finding runs through `pending(…)` (or has a `.pending` file):
  it passes while the bug is there and fails once it is fixed, so the wrapper is removed with the fix.

---

## 8. End-to-end tests

### 8.1 The `k` binary

`crates/k_cli/tests/cli.rs` runs the built `k` binary (`env!("CARGO_BIN_EXE_k")`) in temporary
directories and asserts exit codes (0 = success, 1 = problems found, 2 = usage or internal error),
stdout/stderr and output formats (`--format json` emits JSON Lines). It also builds a `dist/app` by hand
(the `k` binary plus an appended fixture app) and checks `serve`, `probe`, `SIGTERM` (exit 0) and invalid
configuration (exit 78), and builds `tests/e2e/jobs` with `k build` to check `dist/app migrate` (applies
once, then reports the database up to date), `jobs list [--dead]`, `jobs retry <id>` and their exit
codes.

`crates/k_cli/tests/examples.rs` builds **each example** with `k build --out <temp>`, starts its
`dist/app` on a free port (with `DATABASE_URL=sqlite::memory:` and placeholder values for the required
configuration), and checks its main page and one rpc. The static-only `todos` and `kanban` must build to
`dist/public` with an `index.html` and no `dist/app`; their `dist/public` is served by the static host,
and the document (and, for kanban, a deep link) and every hashed asset it links must answer with their
cache headers; kanban's `nginx.conf` and `Dockerfile` are checked to serve the same way. Then weather (the server-rendered home page and an asset; it has no rpc), notes (`/` redirects to
`/sign-in`, which renders; `IsEmailAvailable` answers from the migrated database), shop (the `shop` member
of `examples/shop`: the storefront renders from the database; `Products` and a typed `notFound`) and
paybox (its member: `/health`, and `Payments` refusing a call without a service token with 401). Each app
must stop with exit 0 on `SIGTERM` and log no errors. A database that can't be opened exits 1 before the port opens, without the password.

`crates/k_cli/tests/workspaces.rs` covers workspaces end to end on copies of `examples/stack`
([40-workspaces-and-services.md](../design/40-workspaces-and-services.md)): `k check` at the root and in
members (the libraries mounted), manifest problems and library errors reported once with real paths;
`k test` at the root running every member's tests; `k build` at the root writing each app to
`dist/<name>/` and `rpc.lock` (which must equal the committed one), then a breaking contract change
failing `k check` until both protocols carrying the type are `@Breaking`, and the next build keeping the
field number; the two built services calling each other with signed tokens (a typed error crossing both
hops, a wrong secret mapped to the shop's own error, a call without a token refused with 401); and `k dev`
at the root serving both apps with their remotes wired, so one rpc to the shop makes paybox call the
shop back. On a copy of `examples/shop`, run as its README says (each member's `.env` from its
`.env.example`), `k dev` at the root gives each member its own SQLite database below its own directory,
and an order placed and paid through the shop is marked paid by paybox's event, with the run's one
development secret on both sides.

`crates/k_cli/tests/toolchain.rs` covers **toolchain pinning** ([48-releases-and-versioning.md](../design/48-releases-and-versioning.md#pinning-a-toolchain)) with `K_HOME` pointing at a temporary directory of fake toolchains, `toolchains/99.*/k`, shell scripts that print their version, arguments, working directory, `K_TOOLCHAIN_DELEGATED` and standard input, then exit 7 (Unix only; the tests that need no fake run everywhere). It checks that the locked toolchain runs with identical arguments and streams and its exit status; that without a lock the newest satisfying release is chosen (never a pre-release); that a stale lock yields to the requirement; that `K_TOOLCHAIN_DELEGATED` stops recursion; that a workspace member uses the root's lock; that a missing toolchain exits 2 with `K8201` and its install command, as text and as JSON; that a patch difference runs and `k build` (never `k check`) records it, at the workspace root for a member; that `k lsp` delegates; `K_TOOLCHAIN` in and outside a project; and `K8002` for an unreadable `k.lock`, and `K8202` for one in a newer format, which no `k build` rewrites (a declared `format = 1` survives the toolchain update). The fake versions are `99.*`, so the tests don't change when the toolchain's version does. Requirement parsing and matching are in `crates/k_sema/tests/workspaces.rs` (`requirements::`).

`crates/k_cli/tests/deploy.rs` checks **the deployment files** of `examples/shop` without Docker or a
cluster. It builds the workspace in a copy (whose `rpc.lock` files must equal the committed ones), reads
each app's variables from `dist/<app>/app config`, and parses `docker-compose.yml` (merge keys applied)
and the Kubernetes manifests (`yaml-rust2`), the ConfigMaps behind `envFrom` and the Secrets that
`create-secrets.sh` makes. For every container running an app it requires every required variable, a
secret only as `NAME_FILE` naming a mounted file that comes from a `secrets/` file with a committed
`.example`, no variable the app doesn't read, health probes on `/health`, the images the Dockerfile
builds, the service URLs naming existing services, and the two services' tokens paired (the shop signs
with paybox's `K_SERVICE_TOKEN`, paybox with the shop's, and they differ). It also checks that a
migration Job runs `dist/app migrate` per app, two shop replicas, that both Postgres setups create
paybox's database the same way, that Compose and Kubernetes read the same secret files, all generated by
`scripts/init-secrets.sh`, and that `secrets/` is git- and Docker-ignored. Two rules came from running
the manifests on kind (§8.7): every pod sets `automountServiceAccountToken: false`, and every app
Deployment has a `preStop` sleep of 3–15 seconds and a `terminationGracePeriodSeconds` longer than that
sleep plus `K_SHUTDOWN_GRACE`. `examples/shop/scripts/e2e.sh`
runs the Compose stack itself (it needs Docker): a declined card, a paid order confirmed by paybox's
event, one email in Mailpit, a session shared by the replicas and the sign-in rate limit. Every example's
Compose stack also runs under `cargo test` with `K_COMPOSE=1` (§8.6), and the Kubernetes manifests on
kind with `K_KIND=1` (§8.7).

### 8.2 Server end-to-end tests

`crates/k_server/tests/e2e.rs` starts a real server on an ephemeral port (`127.0.0.1:0`, two workers) and
runs an **HTTP request script** against it. Two kinds of case:

| Kind | Where | Server bundle |
|---|---|---|
| K project | `tests/e2e/<name>/` with `k.toml`, `requests.http`, optional `expected.pending` | analysed (must check cleanly), compiled by `k_codegen` (`Development`), client assets served, split checked; served with the development environment of `k run` (the manifest's config resolved from the project's `.env` only, never the test runner's environment) and the manifest's session-free endpoints as `forgery_exempt` |
| bundle fixture | `crates/k_server/tests/http/<name>/` with `server.js`, `requests.http`, optional `manifest.json` (`{"rpcPaths": [...], "publicOrigin": "..."}`) and `assets/` | the hand-written `server.js` as the entry module; tests the HTTP front and the worker loop without codegen |

**The request script format** (`requests.http`, parsed by `k_server::testing::parse_script`):

```text
# Comments start with '#'. Cookies set by responses are sent on later requests.

### an rpc answers JSON
POST /todos.TodoProtocol/List
content-type: application/json
connect-protocol-version: 1

{}
>>> 200
content-type: application/json
cache-control: private, no-store
json {"value": []}

### the home page renders on the server and escapes
GET /?name=%3Cb%3E
>>> 200
~ <h1>Hello, &lt;b&gt;</h1>
!~ <b>
!set-cookie
```

- `### title` starts an exchange. Then the **request line** `METHOD /path?query`, **headers**
  `name: value`, and optionally a blank line and **body** lines (joined with `\n`, trailing blank lines
  dropped) up to the expectation line.
- `>>> STATUS within 5s` sends the request again every 50 ms until every expectation holds or the time
  (at most 60 s) has passed: for effects that follow a response, such as a background job's.
- `>>> STATUS` starts the **expectations**: `name: value` (the response header equals `value`; names are
  case-insensitive), `!name` (the header is absent), `= text` (the body equals the text; consecutive `=`
  lines join with `\n`; a bare `=` is an empty line), `~ text` (the body contains it), `!~ text` (it
  doesn't), `json VALUE` (the body is JSON equal to `VALUE`).
- Requests are HTTP/1.1 with `Connection: close`; `Host` and `Content-Length` are added. A `Set-Cookie`
  name/value is remembered and sent as `Cookie` on later exchanges of the same script.
- A failure lists every failed expectation with the exchange's line and title. A K-project case with
  `expected.pending` passes while it fails and fails once it passes. All fifteen projects (`hello`, `ssr`,
  `ssr-client-rendering`, `routing`, `rpc-wire`, `request-scope`, `endpoints`, `config-secrets`,
  `auth-sessions`, `interface-bindings`, `passwords`, `http-client`, `email`, `jobs`, `in-process-rpc`)
  pass for real; none is pending. The server runs the jobs worker for a project with jobs, polling
  every 20 ms.
- `K_E2E_LOG=1 cargo test -p k_server --test e2e` prints the workers' log lines (panics with their
  stacks) instead of capturing them.

### 8.3 Fake third parties

`k_server::fakes` provides fakes on ephemeral loopback ports, each on its own thread and recording what
it receives: `FakeHttp` (canned `FakeRoute`s by method and path, optionally delayed; anything else is
`404 no fake route`) and `FakeSmtp` (enough ESMTP for a client without TLS; it accepts every message
except to `reject@…`, answered `550`, and `defer@…`, answered `451`, or follows a fixed `SmtpBehavior`).
`GET /_fake/requests` on a `FakeHttp` answers its recorded requests as JSON, and `GET /_fake/mail` the
messages of the `FakeSmtp` attached with `with_mail`, so K code can read them through its `HttpClient`.

An e2e project with a **`fakes.toml`** gets them for its run; their addresses replace `{fake.http}` and
`{fake.smtp.port}` in its `.env`:

```toml
[http]
routes = [
    { method = "GET", path = "/v1/forecast", body = '{"temperature_2m": 21.5}', content_type = "application/json" },
    { method = "GET", path = "/v1/slow", body = "late", delay_ms = 3000 },
    { method = "GET", path = "/v1/down", status = 503, body = "Service Unavailable" },
]

[smtp]
behavior = "accept"   # or "reject" or "defer"
```

`tests/e2e/http-client` and `tests/e2e/email` use them; `crates/k_server/tests/outbound.rs` tests the ops
against them directly, and `crates/k_server/tests/shop_production.rs` runs the shop's optional Stripe
binding (the shop built for production with `App.k` edited exactly as its documentation shows) against a
fake Stripe (`FakeHttp`) and a fake mail server (`FakeSmtp`): checkout, Stripe failures, the signed
webhook, the confirmation job and its email; and checks that the default build reads no Stripe variable
and requires paybox's. `crates/k_server/tests/sessions.rs` runs one or two
servers on a SQLite file for sessions that survive restarts and are shared by replicas.

`crates/k_server/tests/server.rs` covers what a script can't: streamed bodies over the limit, many
concurrent clients, graceful shutdown with a request in flight, and start-up failures.
`crates/k_server/tests/stage3_server.rs` covers the generated server beyond scripts (cookie attributes,
fixation, `x-k-session`, `Retry-After`, `k-data` as JSON, streaming envelopes, body limits, the split on
served assets, the config table, `dist/app` built by `k build` and run as a process); its harness
serves projects like the e2e one. One test there is pending (stream back-pressure, see server.md §16).
`crates/k_server/tests/channel_server.rs` drives the channels' WebSocket (server.md §20) with a raw
`tungstenite` client (pinned exactly) against `tests/projects/channels` on two workers and a SQLite
database: open and snapshot, acknowledgement after commit, refusals, in-order handling with
per-channel state, resume without loss or duplicates, the handshake's `Origin` and subprotocol checks,
the size, rate, socket and session limits, heartbeats and the silence close, buffer overflow, the
resume window, `Stream.merge`, session end and shutdown. Each test shortens the limits it exercises
through `ServerOptions::channels`.
- **Real browser:** with a Chrome or Chromium executable available, `crates/k_cli/tests/chrome.rs` drives
  it through the DevTools protocol from Rust (§4.4); the tests skip with a message when no browser is
  found, unless `K_REQUIRE_CHROME=1`. `crates/k_cli/tests/firefox.rs` runs the stdlib's crypto,
  web-platform and media checks (`engines.rs`, shared with `chrome.rs`) on Firefox over WebDriver BiDi,
  skipped without Firefox unless `K_REQUIRE_FIREFOX=1`.
- **Example apps:** each app under `examples/` has its own K tests and is built and smoke-tested by the
  e2e suite (§8.1). `k test` runs both sides of them (§8.4), and every example's own tests pass:
  `todos`, `weather`, `notes`, `huddle`, `kanban`, and every member of the workspaces `shop` and `stack`.
  `examples.rs` also builds huddle's `dist/app` and talks to a room over a raw WebSocket.

### 8.4 `k test` and the project commands

`crates/k_cli/tests/commands.rs` runs `k routes`, `k migrate`, `k test` and `k build --out` and compares
their output byte for byte with golden files in `crates/k_cli/tests/golden/` (hand-written first; a
format change is a reviewed diff):

- **`k routes`** on `examples/shop/shop` (endpoint pages, methods, guards, path parameters),
  `examples/shop/paybox` (endpoints only) and `examples/notes` (query parameters with defaults), at a
  workspace root (exit 2, naming the members), and on a fixture with `routes.lock` redirects and an endpoint with two
  methods; a project without pages prints only the header; errors exit 1, a non-project 2.
- **`k migrate new`** on a fixture catalog (the first migration and lock: every column kind, a relation),
  on `examples/notes` taken back to before pinning (an added column with its default: the generated file
  must equal the example's intent), an evolution (new columns, a new table, a retired field), the refusals
  (a required column without a default, a changed type, an enum case) and `--empty`; each generated
  project must then pass `k check`. **`k migrate status`/`apply`** against a SQLite file named by `.env`.
- **`k test`** on the fixture project `crates/k_cli/tests/fixtures/counter` (a client service, a
  component, passing and failing tests: an `expect` with its operand values, an `expect throws` that
  isn't satisfied, a query that matches nothing): the whole report, `--filter`, a filter matching nothing
  (exit 1), a project without tests (exit 0), a project with errors (exit 1, the errors printed), every
  test of `examples/todos`, and `--watch` re-running only the test files a change affects (an edit of
  `CounterView.k` re-runs `CounterView.test.k` only; of `Counter.k`, both).
- **`k check` runs the tests:** on the counter fixture each failure is a `K7510` among the diagnostics,
  human-readable and in JSON Lines, and the summary counts the tests that ran; `--no-tests` skips them;
  a project with errors runs none; `examples/todos` passes with its tests.
- **Every example's own tests** (`test_runs_the_{todos,weather,notes,shop,kanban}_example`; kanban's
  drags with `drag(to:)` and the keyboard, its shortcuts, forms, files through `TestFiles`, the card
  link through `TestClipboard` and progress on the `TestClock`), and `k check` plus a static `k build` of kanban with its feature chunks: all must pass, the
  server-side ones included (sessions, guards, repositories on a fresh database, server streams,
  `TestClient` through endpoint pages and server rendering, `InlineJobQueue` and `RecordingEmailProvider`,
  server-side `use` rules and configs bound by value).
- **Regressions of the runner** (`crates/k_cli/tests/fixtures/regressions`, every test must pass; and
  `fixtures/throws`, whose second test must fail with the case it threw; and `fixtures/sharing`,
  every test must pass: `Clipboard`, `Share`, `CopyButton` and `ShareButton` against `TestClipboard`
  and `TestShare`; and `fixtures/locating`, every test must pass: `Geolocation` against
  `TestGeolocation`, the prompt from a click, refusals, no fix, and a `watch` in an effect ending when
  the permission is revoked; and `fixtures/environment`, every test must pass: the environment services
  against their doubles and the element events driven from handles; and `fixtures/tabs`, every test must
  pass: `TabMessaging` against `TestTabMessaging` through a service's `listen` and an effect's
  `messages`, and `KeepScreenAwake` against `TestWakeLock`; and `fixtures/notifying`, every test must
  pass: `Notifications` against `TestNotifications`, the prompt from a click, a show that never prompts,
  refused, dismissed and blocked permissions, a click opening the notification's page, and a
  `SystemNotification` with its page through JSON; and `fixtures/pushing`, every test must pass:
  `PushSubscriptions` against `TestPushSubscriptions` and `WebPush` against `RecordingWebPush` (the prompt
  from a click, subscribing with the server's key, a refused, dismissed or unsupported browser, a server
  without a key, expired subscriptions, failures, `PushSubscription` through JSON), and the same fixture under
  `k run` against `push.rs`' fake push service; and `fixtures/passkeys`, every test must pass: `Passkeys`
  against `TestAuthenticator` (real CBOR, keys and signatures from `op_k_passkey_test_register` and
  `_assert`) and the real `PasskeyVerifier`: sign-up and sign-in with ES256, EdDSA and RS256, a dismissed
  sheet, an unsupported browser, another origin, no user verification, a used, replaced or other-ceremony
  challenge, a regressed counter, a swapped user handle, a forged signature and an excluded credential,
  and the same fixture under `k run` in Chrome with DevTools' virtual authenticator; and `fixtures/collections`,
  every test must pass: `Collections` against `MemoryCollections` (keys and dates in IndexedDB's order,
  ranges, replacing, deleting, a value an older version stored, simulated failures, persistence, the
  estimate, a collection and an index redeclared by test code), its `k build` writing `storage.lock` and
  `k check` refusing a changed key field (K8009), and the same fixture built and served statically in
  Chrome: values across a reload, an upgrade that adds an index (`Topics.k.v2`) over stored values, and two
  tabs of two builds sharing the database; and `fixtures/realtime`, every test must pass: channels
  in-process through `TestChannels` (resume without loss, a restart with a new snapshot, offline, hours
  of heartbeats, `open`), `TestPeers` (a second browser posting, refused, a guest, sessions ended by
  `endAll` and by the peer's own sign-out) and `TestChannel` for a service alone; and `fixtures/feeds`, every
  test must pass: raw WebSockets against `TestWebSocketClient` and `TestWebSocketServer` (a page following a
  feed, a reconnect after `Backoff`'s delay on the `TestClock`, unmounting closing the socket, a server service
  sharing the test's double, a refusal, `Backoff` and `WebSocketMessage.json()`)): each bug of the test bundle or
  the kit that an example exposed, pinned before it was fixed: a chained enum page parameter given as an
  implicit member, `Page.x` with only optional parameters, a group helper named like the tested
  service's function, a labelled argument after a default, `expect throws` checking the case, a
  group named after a declaration whose file sorts after the group's (it took the declaration's name).
- `K_TEST_BUNDLE_DIR=<dir> k test` writes the test bundle's modules there, for debugging the compiler.
- **`k build --out`** removes the hashed assets of earlier builds and keeps files outside
  `public/assets/`.

The runner's pieces have their own tests: the kit's JS tests (`js/tests/kit.test.js`, runtime.md §13),
the `tests/corpus/sema/app/jobs/storage-lock-*` cases for K8002/K8003, `dist::tests` and
`host::tests::file_dependencies_are_the_files_a_file_refers_to`.

**Server side.** `k test` runs one browser and one server in one isolate (codegen.md §21.1, runtime.md
§13): rpcs are answered in-process through the wire format, server-only services and repositories
injected by a test come from its server container, `SessionManager` acts on the test's one session, and
every test gets a fresh in-memory database with the migrations applied. The pieces have their own tests:
`server-in-tests.test.js` (the request's test context, the virtual clock, `perTest`, the session hook, a
streamed rpc's sink, `TestClient`, endpoint URLs), `kit.test.js` (the run with a server, the watchdog of
a failed test, natives), `http-response.test.js`, and the engine's `op_k_test_reset`, `op_k_test_tick`
and virtual-time rate limits (`crates/k_server/tests/engine.rs`).

### 8.5 Language server

`crates/k_cli/tests/lsp.rs` spawns `k lsp` and drives it over JSON-RPC on its standard input and output
against the example apps: the lifecycle and capabilities, malformed input, definition and type definition,
implementation (shop's `PaymentGateway`, notes' `NotesProtocol`), references and rename (with the file
rename), hover (reactive dependencies, page URLs, docs), completion after an edit, signature help, symbols,
diagnostics and quick fixes as a document changes, and inlay hints. Positions are found by searching the
sources, and edits are made to open documents only, never to the examples on disk.

`crates/k_lsp/tests/server.rs` runs the server in process over channel-backed streams (the driver in
`tests/support/driver.rs`): exit codes, requests after `shutdown`, malformed frames, synchronization with the
disk and watched files, loose projects, definition into the stdlib, and go-to-definition at every identifier
of every example, which must answer a declaration spelled like the identifier.
`crates/k_lsp/tests/properties.rs` feeds random edits (UTF-16 ranges, sometimes outside the document, K
fragments, non-ASCII text) and requests (some cancelled) through the server and checks one well-formed
response per request and that the published diagnostics converge on a fresh check of the edited text; the
`lsp_edits` fuzz target runs the same driver. `crates/k_lsp/tests/editors.rs` checks the editor grammars
against the lexer's keyword table ([lsp.md §10](lsp.md#10-testing-and-budgets)).

### 8.6 Compose smoke tests

`crates/k_cli/tests/compose.rs` runs the examples' Docker Compose stacks as their READMEs do, which the
agreement tests above (`deploy.rs`, `call_deploy.rs`, the compose checks in `examples.rs`) only read:

```text
K_COMPOSE=1 cargo test -p k_cli --test it -- compose:: --test-threads 1
```

| Test | Stack | Flow |
|---|---|---|
| `shop_stack_…` | `examples/shop/docker-compose.yml`: Postgres, Mailpit, paybox, two shop replicas, Caddy | the demo catalog loaded with `psql`, an order placed and paid over HTTP, paybox's event turning the receipt paid, one email in Mailpit; in Chrome, a mug added to the cart and bought with the card form |
| `kanban_stack_…` | `examples/kanban/docker-compose.yml`: nginx serving `dist/public` | the document (`no-cache`), a deep link, a hashed asset (`immutable`), a missing asset (404); in Chrome, under nginx's Content-Security-Policy, a board and a card created and kept across a reload |
| `huddle_stack_…` | `examples/huddle/docker-compose.yml`: Postgres, `app1`, `app2`, nginx | two people signed in through nginx; the second one's channel socket is reopened until `k_presence` shows it on the other replica; a message posted by one reaches the other (Postgres `NOTIFY`) |
| `call_stack_…` | `examples/call/compose.yaml`: the app and coturn | the sign-in page and its script, `iceServers` naming coturn with a signed credential, coturn answering a STUN binding request over UDP, its TCP port open |

- **Gating.** Without `K_COMPOSE=1` each test prints `skipped: …` and passes. With it, a missing Docker
  (or Compose v2) fails the test, as `K_REQUIRE_DOCKER=1` does elsewhere. The Chrome parts are skipped
  without Chrome unless `K_REQUIRE_CHROME=1` (§4.4).
- **Every stack checks** `/_k/health` (`ready`; kanban has no server, so its image's own health check,
  which `--wait` waits for, stands in), the home page, and at the end that every container is running
  and was never restarted (`restart: unless-stopped` hides a crash from `--wait`) and that no K app
  logged an error.
- **Secrets** are generated for the run in a scratch directory below the test target directory, never in
  `secrets/`: the shop's by `scripts/init-secrets.sh <dir>` (with `K` set to the test's `k` binary for
  `k hash-password`), call's as random values in place of `secrets/turn_secret.txt.example`. Every secret
  of the Compose file must have a committed `.example` template. The directory is deleted when the test
  ends, failed or not.
- **The override** (a second `--file`) changes only what a test run must: every published port is one in
  5200–5999 bound to `127.0.0.1` (`ports: !override`), `K_PUBLIC_ORIGIN` names it, the secrets point at
  the scratch files, the built images are tagged `k-compose-test-*:local` so an image built by hand is
  never replaced, and the shop's network moves to `172.29.189.0/24` (with Caddy's address and
  `K_TRUSTED_PROXIES`) so a stack started by hand keeps its subnet. coturn, which uses host networking in
  `compose.yaml`, joins the project network and publishes its listening port, UDP on every interface since
  Docker Desktop doesn't forward UDP published on `127.0.0.1` alone.
- **Cleanup.** The project name is `k-compose-test-<example>-<pid>-<ms>`. Dropping the stack, also on a
  panic (after printing the services' logs), runs `docker compose down --volumes --remove-orphans` and
  removes the `k-compose-test-*` images; each test removes only what its own project created. A stack
  left by a killed run (its process id is gone) is taken down by the next run. Base images are the ones
  the Dockerfiles and Compose files name; nothing is pruned. Stacks run one at a time.
- **Build time.** Every image compiles the `k` toolchain from this repository in a `rust` stage (38,
  "The toolchain image"). The first run on a machine pays for that release build of `k_cli`, which is the
  bulk of the time (not timed here from a cold cache; expect ten minutes or more); the stage is shared by all four
  Dockerfiles and Cargo's registry and target directories are BuildKit cache mounts, so later runs
  rebuild only what changed. With a warm cache the four tests take about a minute in all (measured
  2026-11 on an Apple-silicon laptop: kanban 8 s, call 10 s, huddle 20 s, shop 30 s to healthy, plus the
  flows). `up` waits up to 40 minutes.

### 8.7 Kind smoke test

`crates/k_cli/tests/kubernetes.rs` runs `examples/shop/deploy/kubernetes`, which `deploy.rs` only reads,
on a real cluster:

```text
K_KIND=1 cargo test -p k_cli --test it -- kubernetes:: --test-threads 1
```

It needs Docker, [kind](https://kind.sigs.k8s.io) and `kubectl` (`K_KUBECTL` names one that isn't first
on `PATH`). Without `K_KIND=1` it prints `skipped: …` and passes; with it, a missing tool fails it.

| Step | What happens |
|---|---|
| Cluster | `kind create cluster` from `kind-config.yaml`, named `k-kind-test-<pid>-<ms>`, with its own kubeconfig in the scratch directory (the reader's contexts are untouched); every `hostPort` moves to a free port in 5200–5999, and one more mapping reaches a test `NodePort` Service |
| Images | `docker build` with `examples/shop/Dockerfile`, as Compose builds them, tagged `k-kind-test-{paybox,shop}:local`, then `kind load docker-image`; no registry. Postgres and Mailpit are pulled by the node (`kind load` of a multi-platform image from Docker Desktop's image store fails with `content digest … not found`) |
| Secrets | `scripts/init-secrets.sh <scratch>` and `create-secrets.sh <scratch>`, the README's own scripts |
| Apply | an overlay in the scratch directory (a symlink to the manifests as its base: kustomize takes no absolute base) renames the images, sets `K_PUBLIC_ORIGIN` to the test port and adds the `NodePort` Service; then `kubectl wait` for both migration Jobs to complete and the Deployments to be Available |
| Flow | `/_k/health` is `ready`; the demo catalog loaded with `kubectl exec … psql`; the home page lists it; an order placed (the shop calling paybox's Service) and paid (paybox's event calling the shop's Service back) until the receipt says so |
| Rolling restart | `/_k/health` asked on a new connection every 20 ms through the Service while `kubectl rollout restart deployment/shop` replaces both pods and the old ones drain; every answer must be 200. Then another order, no pod restarted during the test, and no app logged an error |

- **No ingress controller is installed:** that would pull images from outside the machine. The Ingress is
  applied and must be accepted; the `NodePort` Service stands in for the controller, and like it goes
  through kube-proxy, so a pod leaving the Service is seen as a client would see it. `kubectl
  port-forward` wouldn't do: it pins one pod and breaks when that pod stops.
- **Cleanup.** Dropping the cluster guard, also on a panic (after printing the pods, events and logs),
  runs `kind delete cluster`, removes the two test images and deletes the scratch directory with the
  secrets. A cluster of this naming scheme whose process id is gone is deleted by the next run; no other
  cluster is touched.
- **Time.** About 90 seconds with a warm Docker build cache (measured 2026-11 on an Apple-silicon laptop:
  the cluster 27 s, both images 9 s, migrated and available 41 s, the rolling restart 12 s); a cold
  cache adds the toolchain build of §8.6.
- **What it found** (38-deployment.md, "As built (kind)"): Postgres never started, because the API token
  mounted at `/var/run/secrets/kubernetes.io` needed a directory inside the read-only Secret volume at
  `/run/secrets`; and a rolling restart refused about one connection in four hundred, because `serve`
  stops accepting on `SIGTERM` before kube-proxy has removed the pod. `deploy.rs` now requires
  `automountServiceAccountToken: false` on every pod, and on each app Deployment a `preStop` sleep and a
  grace period longer than the sleep plus `K_SHUTDOWN_GRACE`.

---

### 8.8 Compatibility fixtures

```sh
cargo test -p k_server --test it -- compat::
K_BLESS=1 cargo test -p k_server --test it -- compat::wire::   # re-record the current minor
```

The suite behind [design 48](../design/48-releases-and-versioning.md)'s promise that formats change only
with a version and that adjacent minors understand each other ([release.md §4](release.md#4-for-contributors-breaking-and-format-changes)).

- **`compat::locks::`** mounts golden lockfiles next to the fixture program `tests/compat/locks/project/`:
  `tests/compat/locks/<file>/<format>/<file>` for each format of `k.lock`, `rpc.lock`, `schema.lock`,
  `storage.lock` and `routes.lock`. It checks there is one golden per format up to the current one
  (`k_sema::lockformat::FORMATS`), that each is read with no diagnostic and written back unchanged, that
  a stale lock is rewritten exactly to the golden with and without a declared `format = 1`, that a
  format above the current one is `K8202` at its line and never rewritten, and that a `format` that
  isn't a number is `K8002`. The diagnostics are `k check`'s (`k_sema` plus
  `k_codegen::project_diagnostics`) and the writers `k build`'s.
- **`compat::wire::`** replays `tests/compat/wire/<minor>/<example>.json` for every minor: `paybox`
  (`examples/shop/paybox` on a temporary SQLite database, with a fake merchant from `k_server::fakes`)
  and `notes` (`examples/notes`). Each step's recorded request is sent to the current build (ids mapped
  from the recorded responses to this run's, service tokens signed again), and what comes back is
  compared with the recording: status, `content-type`, `cache-control` and `set-cookie`, the body, a
  stream's envelopes, the `k_job` row (then replaced by the recorded payload and run by the worker),
  the call to the merchant, and the page's `k-data`. Ids, instants, ports, cookie values and token MACs
  are compared by shape, and a response may add fields but not drop or change one. The client half
  loads the generated browser client in V8 with the fake DOM: each recorded rpc response goes through
  the stub the app calls, the recorded envelopes through its stream reader, and the recorded `k-data`
  starts the app, which must stay signed in and show the note the recorded stream carries.
  `K_BLESS=1` records only the current minor; older minors are never rewritten.

## 9. Benchmarks and performance budgets

Benchmarks use [criterion](https://docs.rs/criterion) and live in `crates/<crate>/benches/`. They report
throughput in bytes per second where that is meaningful, so results compare directly with the budgets in
[architecture.md](architecture.md#12-performance-budgets).

```sh
cargo bench -p k_syntax                              # run
cargo bench -p k_syntax -- --save-baseline main      # record a baseline
cargo bench -p k_syntax -- --baseline main           # compare a change against it
cargo bench -p k_sema                                # the semantic benchmarks
cargo bench -p k_codegen                             # code generation
```

`crates/k_syntax/benches/syntax.rs` measures, in lines/s and bytes/s, lexing and parsing of realistic
generated K (`lex`, `parse`: 1k, 10k and 100k lines; `parse_only`: the parser and tree sink alone), of one
typical 300-line file (`typical_file`), and of fifteen pathological families at about 1 MB each
(`pathological_lex`, `pathological_parse`: deep nesting at and past the limit, very long lines and strings,
huge and deeply nested interpolations, an error on every line, comments, non-ASCII text, long member and
operator chains). All input comes from the deterministic generator in `crates/k_syntax/benches/workload/`,
whose output is itself tested (`crates/k_syntax/tests/workload.rs`: realistic input is valid K for many
seeds, covers every declaration kind, and each pathological family has its documented shape).

`crates/k_sema/benches/sema.rs` measures semantic analysis (`AnalysisHost` + `diagnostics()`, parsing
included) of generated multi-module projects: a cold check of 1k, 10k and 100k lines on one thread
(`cold_1t`) and on every core (`cold_mt`), a warm re-check after a function body edit (`warm_body`) and
after a public signature edit (`warm_signature`), and four pathological families (`pathological`: deep
generic nesting, a 2,000-case `match`, a 1,000-module import chain, a 1,000-service DI graph). Its
generator, `crates/k_sema/benches/project/`, is tested in `crates/k_sema/tests/project_workload.rs`: every
project, family and benchmark edit analyses with zero diagnostics, sizes are exact and generation is
deterministic.

`crates/k_codegen/benches/codegen.rs` measures code generation over the same generated projects:
lowering, printing and bundling of an already checked 1k, 10k and 100k-line project (`emit`, in lines of
K per second) and a cold build, check included (`build`).

**Budget check.** `scripts/bench-check.sh` runs the ignored `perf_budget` tests of every area in a release
build, one test at a time, and fails when any budget is missed (`--syntax`, `--sema`, `--codegen`,
`--runtime`, `--server`, `--db`, `--app`, `--lsp` select areas):

| Area | Test file | Checks |
|---|---|---|
| syntax | `crates/k_syntax/tests/perf_budget.rs` | lexing, parsing, the typical file, pathological floors, linearity |
| sema | `crates/k_sema/tests/perf_budget.rs` | cold and warm `k check` of 100k lines, memory, linearity |
| codegen | `crates/k_codegen/tests/perf_budget.rs` | lowering + printing of 100k lines, cold build, linearity |
| runtime | `crates/k_runtime/tests/runtime.rs` | client runtime size (raw, compacted, gzip) |
| server | `crates/k_server/tests/perf_budget.rs` | in V8: signal propagation, signal-to-DOM, keyed lists of 1k/10k rows, hydration; over HTTP in process: rpc latency and throughput, SSR latency and throughput, SQLite rpcs, cold start |
| db | `crates/k_db/tests/perf_budget.rs` | query rendering, driver overhead over `rusqlite` |
| app | `crates/k_cli/tests/perf_budget.rs` | the real `dist/app` of the examples: cold start, SSR and database rpcs over HTTP, shipped asset sizes |
| lsp | `crates/k_lsp/tests/perf_budget.rs` | hover, definition, references and completion warm on 100k lines; the first hover after an edit; diagnostics after a keystroke end to end |

Methods, baselines and history are in [performance.md](performance.md). New hot paths get a benchmark when
they are written. A change that makes a benchmark more than 5% slower needs a stated reason.

---

## 10. Planned suites

These suites are added to `k_corpus` together with the stages they test. Their formats are fixed now so
tests can be written early.

| Suite | Runs | Expectations |
|---|---|---|
| `fmt` (deferred) | the formatter | `name.formatted.k` |

The semantic suite planned here as `check` exists as `sema` (§3.8); `codegen` and `run` exist (§3.9,
§3.10).
