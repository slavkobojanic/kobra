#!/usr/bin/env bash
# Checks the toolchain and the built product against their performance
# budgets.
#
# Runs the ignored `perf_budget` tests of each area (the `perf_budget`
# module of each crate's test binary `it`) in an optimized build,
# one test at a time so measurements do not compete for cores:
#
#   syntax   crates/k_syntax/tests/perf_budget.rs   lexer and parser
#   sema     crates/k_sema/tests/perf_budget.rs     k check (cold, warm, memory)
#   codegen  crates/k_codegen/tests/perf_budget.rs  lowering + printing, cold build
#   runtime  crates/k_runtime/tests/runtime.rs      client runtime size (raw, gzip)
#   server   crates/k_server/tests/perf_budget.rs   signals, keyed lists, hydration,
#                                                   SSR, rpc, SQLite, cold start (in process)
#   db       crates/k_db/tests/perf_budget.rs       query rendering, driver overhead
#   app      crates/k_cli/tests/perf_budget.rs      dist/app: cold start, SSR and rpc
#                                                   over HTTP, shipped asset sizes
#   lsp      crates/k_lsp/tests/perf_budget.rs      language server: hover, definition,
#                                                   references, completion, diagnostics
#                                                   after a keystroke
#
# The semantic memory check runs in a process of its own, so no other
# test's freed memory is counted. Every check runs even when an earlier one
# fails; the script exits non-zero if any budget in
# docs/compiler/architecture.md ("Performance budgets") or any linearity
# check is missed. Numbers are printed so CI logs record them; the
# baselines live in docs/compiler/performance.md.
#
# Usage:
#   scripts/bench-check.sh                  # every budget check
#   scripts/bench-check.sh --criterion      # also run the criterion benchmarks
#   scripts/bench-check.sh --syntax --sema  # only the named areas (any of
#                                           # --syntax --sema --codegen --runtime
#                                           # --server --db --app --lsp)
#
# Environment:
#   K_BUDGET_SCALE=0.5   enforce half the throughput budget (slower machines);
#                        see the documentation of each perf_budget.rs.
set -euo pipefail

cd "$(dirname "$0")/.."
if [ -f "$HOME/.cargo/env" ]; then
    # shellcheck disable=SC1091
    . "$HOME/.cargo/env"
fi

areas=""
criterion=0
for arg in "$@"; do
    case "$arg" in
        --criterion) criterion=1 ;;
        --syntax | --sema | --codegen | --runtime | --server | --db | --app | --lsp)
            areas="$areas ${arg#--}"
            ;;
        *)
            echo "usage: $0 [--criterion] [--syntax] [--sema] [--codegen] [--runtime] [--server] [--db] [--app] [--lsp]" >&2
            exit 2
            ;;
    esac
done
if [ -z "$areas" ]; then
    areas="syntax sema codegen runtime server db app lsp"
fi

selected() {
    case " $areas " in
        *" $1 "*) return 0 ;;
        *) return 1 ;;
    esac
}

budget() {
    # $1: package, rest: libtest arguments (by default the `perf_budget`
    # module of the package's test binary `it`).
    local package="$1"
    shift
    [ $# -eq 0 ] && set -- perf_budget::
    cargo test --release -p "$package" --test it -- \
        --ignored --nocapture --test-threads=1 "$@"
}

status=0
if selected syntax; then
    budget k_syntax || status=1
fi
if selected sema; then
    budget k_sema perf_budget:: --skip memory_of_ || status=1
    budget k_sema --exact perf_budget::memory_of_a_warm_100k_line_project || status=1
fi
if selected codegen; then
    budget k_codegen || status=1
fi
if selected runtime; then
    # Not ignored: the size budgets are part of the normal test suite too.
    cargo test --release -p k_runtime --test it -- --nocapture size_budget || status=1
fi
if selected server; then
    budget k_server || status=1
fi
if selected db; then
    budget k_db || status=1
fi
if selected app; then
    budget k_cli || status=1
fi
if selected lsp; then
    budget k_lsp || status=1
fi

if [ "$criterion" = 1 ]; then
    for crate in syntax sema codegen; do
        if selected "$crate"; then
            cargo bench -p "k_$crate" || status=1
        fi
    done
fi
exit "$status"
