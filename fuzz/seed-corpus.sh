#!/bin/sh
# Seeds the libFuzzer corpora of every fuzz target from the golden corpus.
#
# Usage (from the repository root or from fuzz/):
#
#     fuzz/seed-corpus.sh
#
# Copies every `tests/corpus/**/*.k` file into `fuzz/corpus/<target>/`
# (flattened, named after its corpus path), plus the regression inputs in
# `tests/corpus/**/regressions/`. Existing corpus entries are kept, so the
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
for target in $targets; do
    count=$(find "$here/corpus/$target" -type f | wc -l | tr -d ' ')
    echo "fuzz/corpus/$target: $count inputs"
done
