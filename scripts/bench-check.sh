#!/usr/bin/env bash
# Checks the syntax layer and semantic analysis against their performance
# budgets.
#
# Runs the ignored tests in crates/k_syntax/tests/perf_budget.rs and
# crates/k_sema/tests/perf_budget.rs in an optimized build, one at a time so
# measurements do not compete for cores. The semantic memory check runs in a
# process of its own, so no other test's freed memory is counted. Every
# check runs even when an earlier one fails; the script exits non-zero if
# any budget in docs/compiler/architecture.md ("Performance budgets") or any
# linearity check is missed. Numbers are printed so CI logs record them; the
# baselines live in docs/compiler/performance.md.
#
# Usage:
#   scripts/bench-check.sh             # the budget check
#   scripts/bench-check.sh --criterion # also run the criterion benchmarks
#   scripts/bench-check.sh --syntax    # only the syntax budgets
#   scripts/bench-check.sh --sema      # only the semantic budgets
#
# Environment:
#   K_BUDGET_SCALE=0.5   enforce half the throughput budget (slower machines);
#                        see the documentation of perf_budget.rs.
set -euo pipefail

cd "$(dirname "$0")/.."
if [ -f "$HOME/.cargo/env" ]; then
    # shellcheck disable=SC1091
    . "$HOME/.cargo/env"
fi

syntax=1
sema=1
criterion=0
for arg in "$@"; do
    case "$arg" in
        --criterion) criterion=1 ;;
        --syntax) sema=0 ;;
        --sema) syntax=0 ;;
        *)
            echo "usage: $0 [--criterion] [--syntax | --sema]" >&2
            exit 2
            ;;
    esac
done

status=0
if [ "$syntax" = 1 ]; then
    cargo test --release -p k_syntax --test perf_budget -- \
        --ignored --nocapture --test-threads=1 || status=1
fi
if [ "$sema" = 1 ]; then
    cargo test --release -p k_sema --test perf_budget -- \
        --ignored --nocapture --test-threads=1 --skip memory_of_ || status=1
    cargo test --release -p k_sema --test perf_budget -- \
        --ignored --nocapture --exact memory_of_a_warm_100k_line_project || status=1
fi

if [ "$criterion" = 1 ]; then
    if [ "$syntax" = 1 ]; then
        cargo bench -p k_syntax || status=1
    fi
    if [ "$sema" = 1 ]; then
        cargo bench -p k_sema || status=1
    fi
fi
exit "$status"
