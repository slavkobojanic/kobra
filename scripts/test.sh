#!/usr/bin/env bash
# Runs the test suite fast (docs/compiler/testing.md, "Running tests fast").
#
# 1. Builds every test binary once (`cargo test --no-run`).
# 2. Runs three lanes at the same time and waits for all of them:
#      nextest     every test as a process of its own, all binaries in
#                  parallel (.config/nextest.toml);
#      in-process  the suites of thousands of tiny cases (the golden corpus,
#                  the runtime's JS tests, the execution corpus), each binary
#                  running its cases in one process, which is ~100x cheaper
#                  than a process per case;
#      doc         the doc tests (`cargo test --doc`), started once nextest
#                  has finished building, so the two cargo commands don't
#                  wait on each other's lock on the target directory.
# 3. Prints how long each lane and the whole run took, and exits non-zero
#    when any lane failed.
#
# Usage:
#   scripts/test.sh                    # everything
#   scripts/test.sh --ci               # nextest's `ci` profile (longer limits, JUnit)
#   QUICK=1 scripts/test.sh            # fewer property-test and stress cases
#   scripts/test.sh -p k_sema -p k_cli # only these packages
#   scripts/test.sh -E 'test(warm)'    # a nextest filterset on top
#
# scripts/affected-tests.sh chooses the packages, filter, in-process suites
# and doc-test packages for a change and calls this script with them:
#   --in-process "k_corpus::corpus k_server::runtime_js"   ("" for none)
#   --doc "k_sema k_syntax"                               ("" for none)
#
# QUICK=1 sets PROPTEST_CASES=16 and K_STRESS_CASES=500 unless they are set
# already. It is for iterating on a change; CI (`--ci`) refuses it, so the
# full case counts always run there.
#
# Concurrent agents in one checkout: give each its own CARGO_TARGET_DIR
# (docs/compiler/testing.md, "Several agents at once"); cargo serializes
# every build in one target directory.

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT" || exit 2
if [ -f "$HOME/.cargo/env" ]; then
    # shellcheck disable=SC1091
    . "$HOME/.cargo/env"
fi

PROFILE=default
PACKAGES=()
FILTER=()
IN_PROCESS_SET=0
IN_PROCESS="k_corpus::corpus k_server::runtime_js k_server::corpus_run"
DOC_SET=0
DOC=""

while [ $# -gt 0 ]; do
    case "$1" in
        --ci) PROFILE=ci ;;
        --profile) PROFILE="$2"; shift ;;
        -p|--package) PACKAGES+=("$2"); shift ;;
        -E|--filter) FILTER=(-E "$2"); shift ;;
        --in-process) IN_PROCESS="$2"; IN_PROCESS_SET=1; shift ;;
        --doc) DOC="$2"; DOC_SET=1; shift ;;
        -h|--help) sed -n '2,36p' "$0"; exit 0 ;;
        *) echo "scripts/test.sh: unknown argument $1 (see --help)" >&2; exit 2 ;;
    esac
    shift
done

if ! cargo nextest --version >/dev/null 2>&1; then
    echo "cargo-nextest is missing: cargo install cargo-nextest --version 0.9.111 --locked" >&2
    exit 2
fi

if [ "${QUICK:-0}" = 1 ]; then
    if [ "$PROFILE" = ci ]; then
        echo "scripts/test.sh: QUICK=1 is not allowed with --ci (CI runs the full case counts)" >&2
        exit 2
    fi
    export PROPTEST_CASES="${PROPTEST_CASES:-16}"
    export K_STRESS_CASES="${K_STRESS_CASES:-500}"
fi

TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
LOGS="$TARGET_DIR/test-logs/$(date +%Y%m%d-%H%M%S)-$$"
mkdir -p "$LOGS" || exit 2
now() { perl -MTime::HiRes=time -e 'printf "%.1f", time'; }
since() { perl -e "printf '%.1f', $(now) - $1"; }

START="$(now)"

# Packages for cargo: the given ones, else the workspace.
if [ ${#PACKAGES[@]} -gt 0 ]; then
    PKG_ARGS=()
    for p in "${PACKAGES[@]}"; do PKG_ARGS+=(-p "$p"); done
else
    PKG_ARGS=(--workspace)
fi

# Without packages or an explicit list, the in-process suites and the doc
# tests are those of the whole workspace; with packages, only theirs.
in_packages() {
    [ ${#PACKAGES[@]} -eq 0 ] && return 0
    local p
    for p in "${PACKAGES[@]}"; do [ "$p" = "$1" ] && return 0; done
    return 1
}
if [ $IN_PROCESS_SET = 0 ] && [ ${#PACKAGES[@]} -gt 0 ]; then
    kept=""
    for id in $IN_PROCESS; do in_packages "${id%%::*}" && kept="$kept $id"; done
    IN_PROCESS="$kept"
fi
if [ $DOC_SET = 0 ]; then
    if [ ${#PACKAGES[@]} -gt 0 ]; then DOC="${PACKAGES[*]}"; else DOC="all"; fi
fi

# Each crate's integration tests are modules of one binary, tests/main.rs
# (`autotests = false`; docs/design/13-tooling.md, "One test binary per
# crate"), so a test file that is neither a module of it nor a [[test]] of
# its own would never be compiled. Refuse to run without it.
orphans=""
for f in crates/*/tests/*.rs; do
    dir="${f%/tests/*}"
    stem="$(basename "$f" .rs)"
    [ "$stem" = main ] && continue
    grep -Eq "^(pub )?mod $stem;" "$dir/tests/main.rs" 2>/dev/null && continue
    grep -q "^path = \"tests/$stem.rs\"" "$dir/Cargo.toml" && continue
    orphans="$orphans $f"
done
if [ -n "$orphans" ]; then
    echo "scripts/test.sh: not compiled into any test binary (add \`mod <name>;\` to tests/main.rs):$orphans" >&2
    exit 2
fi

# ---------------------------------------------------------------------------
# 1. Build every test binary once.
# ---------------------------------------------------------------------------
echo "== build (${PKG_ARGS[*]})"
BUILD_START="$(now)"
if ! cargo test "${PKG_ARGS[@]}" --no-run --message-format=json-render-diagnostics \
    >"$LOGS/build.json"; then
    echo "build failed" >&2
    exit 1
fi
BUILD_TIME="$(since "$BUILD_START")"

# The executables of the in-process suites, as `binary-id<TAB>exe<TAB>package dir`.
: >"$LOGS/in-process.tsv"
if [ -n "${IN_PROCESS// /}" ]; then
    python3 - "$LOGS/build.json" "$IN_PROCESS" >"$LOGS/in-process.tsv" <<'PY'
import json, os, sys
wanted = set(sys.argv[2].split())
for line in open(sys.argv[1]):
    try:
        m = json.loads(line)
    except ValueError:
        continue
    if m.get("reason") != "compiler-artifact" or not m.get("executable"):
        continue
    t = m["target"]
    if "test" not in t["kind"]:
        continue
    pkg = m["package_id"]
    # package ids look like `path+file:///…/crates/k_server#0.1.0`
    name = os.path.basename(os.path.dirname(m["manifest_path"]))
    for candidate in (name, pkg.split("#")[0].rsplit("/", 1)[-1]):
        bid = f"{candidate}::{t['name']}"
        if bid in wanted:
            print(f"{bid}\t{m['executable']}\t{os.path.dirname(m['manifest_path'])}")
            break
PY
fi

# ---------------------------------------------------------------------------
# 2. The three lanes.
# ---------------------------------------------------------------------------
echo "== run (nextest profile $PROFILE${QUICK:+, QUICK=$QUICK})"

NEXTEST_START="$(now)"
(
    cargo nextest run "${PKG_ARGS[@]}" --profile "$PROFILE" ${FILTER[@]+"${FILTER[@]}"} \
        --no-tests=pass >"$LOGS/nextest.log" 2>&1
    echo $? >"$LOGS/nextest.status"
    perl -e "printf '%.1f', $(now) - $NEXTEST_START" >"$LOGS/nextest.time"
) &
NEXTEST_PID=$!

IN_START="$(now)"
(
    status=0
    while IFS=$'\t' read -r bid exe dir; do
        [ -z "$bid" ] && continue
        t0="$(now)"
        if (cd "$dir" && CARGO_MANIFEST_DIR="$dir" "$exe" -q) >"$LOGS/in-$bid.log" 2>&1; then
            echo "   PASS  $bid ($(since "$t0") s)" >>"$LOGS/in-process.log"
        else
            status=1
            echo "   FAIL  $bid ($(since "$t0") s)" >>"$LOGS/in-process.log"
        fi
    done <"$LOGS/in-process.tsv"
    echo $status >"$LOGS/in-process.status"
    perl -e "printf '%.1f', $(now) - $IN_START" >"$LOGS/in-process.time"
) &
IN_PID=$!

DOC_PID=""
if [ -n "${DOC// /}" ]; then
    (
        # Wait until nextest is past its (no-op) build, so `cargo test --doc`
        # doesn't hold the target directory's lock while nextest needs it.
        for _ in $(seq 1 600); do
            grep -q "Starting\|error:" "$LOGS/nextest.log" 2>/dev/null && break
            [ -f "$LOGS/nextest.status" ] && break
            sleep 0.1
        done
        t0="$(now)"
        if [ "$DOC" = all ]; then
            DOC_ARGS=(--workspace)
        else
            DOC_ARGS=()
            for p in $DOC; do DOC_ARGS+=(-p "$p"); done
        fi
        cargo test "${DOC_ARGS[@]}" --doc >"$LOGS/doc.log" 2>&1
        echo $? >"$LOGS/doc.status"
        perl -e "printf '%.1f', $(now) - $t0" >"$LOGS/doc.time"
    ) &
    DOC_PID=$!
fi

wait $NEXTEST_PID $IN_PID ${DOC_PID:+$DOC_PID}

# ---------------------------------------------------------------------------
# 3. Report.
# ---------------------------------------------------------------------------
FAILED=0
nx_status="$(cat "$LOGS/nextest.status" 2>/dev/null || echo 1)"
echo
echo "== nextest"
if [ "$nx_status" != 0 ]; then
    FAILED=1
    # Failures with their output, then the summary.
    perl -ne 'print if $on ||= /^\s+(FAIL|TIMEOUT|SIG[A-Z]+|LEAK-FAIL) \[/' "$LOGS/nextest.log" | tail -n 400
    grep -E "^error" "$LOGS/nextest.log" | head -20
else
    grep -E "Summary|SLOW|FLAKY|LEAK" "$LOGS/nextest.log" | tail -n 20
fi

echo "== in-process suites"
if [ -s "$LOGS/in-process.tsv" ]; then
    cat "$LOGS/in-process.log" 2>/dev/null
    if [ "$(cat "$LOGS/in-process.status")" != 0 ]; then
        FAILED=1
        for f in "$LOGS"/in-*.log; do
            [ "$f" = "$LOGS/in-process.log" ] && continue
            if grep -q "FAILED\|panicked\|failures:" "$f"; then
                echo "-- $(basename "$f" .log)"
                grep -vE "^test .* \.\.\. ok$" "$f" | tail -n 200
            fi
        done
    fi
else
    echo "   (none selected)"
fi

echo "== doc tests"
if [ -n "$DOC_PID" ]; then
    if [ "$(cat "$LOGS/doc.status" 2>/dev/null || echo 1)" != 0 ]; then
        FAILED=1
        grep -vE "^test .* \.\.\. ok$|^\s*(Compiling|Running|Doc-tests)" "$LOGS/doc.log" | tail -n 200
    else
        passed="$(grep -Eo '[0-9]+ passed' "$LOGS/doc.log" | awk '{s+=$1} END {print s+0}')"
        echo "   $passed passed"
    fi
else
    echo "   (none selected)"
fi

TOTAL="$(since "$START")"
echo
echo "== time: total ${TOTAL} s = build ${BUILD_TIME} s + run (nextest $(cat "$LOGS/nextest.time" 2>/dev/null) s," \
    "in-process $(cat "$LOGS/in-process.time" 2>/dev/null) s, doc $(cat "$LOGS/doc.time" 2>/dev/null || echo -) s, in parallel)"
echo "   logs: $LOGS"
[ "$PROFILE" = ci ] && echo "   JUnit: $ROOT/target/nextest/ci/junit.xml"
if [ $FAILED != 0 ]; then
    echo "== FAILED"
    exit 1
fi
echo "== ok"
