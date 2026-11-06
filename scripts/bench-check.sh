#!/usr/bin/env bash
# Checks the syntax layer against its performance budgets.
#
# Runs the ignored tests in crates/k_syntax/tests/perf_budget.rs in an
# optimized build, one at a time so measurements do not compete for cores.
# Exits non-zero if any budget in docs/compiler/architecture.md ("Performance
# budgets") or any linearity check is missed. Numbers are printed so CI logs
# record them; the baseline lives in docs/compiler/performance.md.
#
# Usage:
#   scripts/bench-check.sh             # the budget check
#   scripts/bench-check.sh --criterion # also run the criterion benchmarks
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

cargo test --release -p k_syntax --test perf_budget -- \
    --ignored --nocapture --test-threads=1

if [ "${1:-}" = "--criterion" ]; then
    cargo bench -p k_syntax
fi
