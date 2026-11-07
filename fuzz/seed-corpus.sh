#!/bin/sh
# Seeds the libFuzzer corpora of every fuzz target from the golden corpus.
#
# Usage (from the repository root or from fuzz/):
#
#     fuzz/seed-corpus.sh
#
# Copies every `tests/corpus/**/*.k` file into `fuzz/corpus/<target>/` of
# the source-text targets (flattened, named after its corpus path), plus the
# regression inputs in `tests/corpus/**/regressions/`, and the request seeds
# of `fuzz/seeds/<target>/` into the server targets' corpora (`http_request`,
# `rpc_json`; the same requests as `RAW_SEEDS` and `RPC_SEEDS` in
# crates/k_server/tests/support/horacle.rs). The generator-driven targets
# (`structured`, `check_structured`, `check_typed`, `codegen`,
# `codegen_run`) also start from the empty input, which generates the
# smallest program. Existing corpus entries are kept, so the
# script can be re-run after adding corpus cases. `fuzz/corpus` is
# git-ignored; minimized crash inputs go to tests/corpus instead (see
# docs/compiler/testing.md, "Fuzzing").
set -eu

here=$(cd "$(dirname "$0")" && pwd)
root=$(dirname "$here")
targets="lex parse parse_roundtrip structured check check_structured check_typed"

for target in $targets; do
    mkdir -p "$here/corpus/$target"
done

find "$root/tests/corpus" -name '*.k' -type f | while read -r file; do
    rel=${file#"$root/tests/corpus/"}
    name=$(printf '%s' "$rel" | tr '/' '_')
    for target in $targets; do
        cp "$file" "$here/corpus/$target/$name"
    done
done
for target in http_request rpc_json; do
    mkdir -p "$here/corpus/$target"
    cp "$here/seeds/$target/"* "$here/corpus/$target/"
done
# Random seeds of 64 to 4096 bytes for the generator-driven stage 3 targets,
# so they start from programs of every size rather than growing from the
# empty input.
for target in codegen codegen_run; do
    mkdir -p "$here/corpus/$target"
    for n in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16; do
        dd if=/dev/urandom of="$here/corpus/$target/random-$n" bs=$((n * 256)) count=1 2>/dev/null
    done
done
for target in $targets http_request rpc_json codegen codegen_run; do
    count=$(find "$here/corpus/$target" -type f | wc -l | tr -d ' ')
    echo "fuzz/corpus/$target: $count inputs"
done
