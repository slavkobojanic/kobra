#!/bin/sh
# Seeds the libFuzzer corpora of every fuzz target.
#
# Usage (from the repository root or from fuzz/):
#
#     fuzz/seed-corpus.sh            # add seeds, keep what the fuzzer found
#     fuzz/seed-corpus.sh --fresh    # also set the semantic targets' grown
#                                    # corpora aside first (see below)
#
# Seeds per target:
#
# * `lex`, `parse`, `parse_roundtrip`, `structured`: every
#   `tests/corpus/**/*.k` file (flattened, named after its corpus path).
#   These targets run thousands of inputs a second, so the whole corpus
#   loads in seconds.
# * `check`: a curated set of about 140 small files: one case per
#   directory of the `sema` suite (the median-sized single-file case of
#   each directory two levels below `tests/corpus/sema`) plus every
#   `regressions` case of the `sema`, `parser` and `lexer` suites, each at
#   most 2 KiB (`CHECK_MAX_INPUT` in fuzz/src/lib.rs). Every `check` input
#   is a whole-project analysis (about 50 a second with sanitizers), so the
#   full corpus (2,600 files) took minutes to load before any fuzzing
#   started; the curated set loads in a few seconds and covers every
#   checker area.
# * `check_structured`, `check_typed`, `codegen`, `codegen_run`,
#   `lsp_edits`: their bytes drive a generator, so Kobra source is no better a
#   seed than noise. They get sixteen random seeds, so they start from
#   programs of every size rather than growing from the empty input: 64 to
#   1,024 bytes for the semantic two (their cap, `GENERATOR_MAX_INPUT`) and
#   `lsp_edits` (whose cap is 2 KiB), 256 to 4,096 for the code generation
#   two.
# * `http_request`, `rpc_json`: the request seeds of `fuzz/seeds/<target>/`
#   (the same requests as `RAW_SEEDS` and `RPC_SEEDS` in
#   crates/k_server/tests/support/horacle.rs).
#
# Existing corpus entries are kept, so the script can be re-run after
# adding corpus cases. A corpus the fuzzer has grown for a long time loads
# slowly again; `--fresh` moves the `check`, `check_structured` and
# `check_typed` corpora to `fuzz/corpus-old/<target>.<seconds>` (nothing is
# deleted; merge back what matters with `cargo fuzz cmin`) before seeding.
# Every target of fuzz/Cargo.toml is in `all_targets` (checked by
# crates/xtask/tests/workflows.rs), and the script fails when one is left
# empty: the release gate runs each target over its seeded corpus in a fresh
# checkout (docs/compiler/release.md §9).
#
# `fuzz/corpus` and `fuzz/corpus-old` are git-ignored; minimized crash
# inputs go to tests/corpus instead (see docs/compiler/testing.md,
# "Fuzzing").
set -eu

here=$(cd "$(dirname "$0")" && pwd)
root=$(dirname "$here")
corpus="$root/tests/corpus"
text_targets="lex parse parse_roundtrip structured"
semantic_targets="check check_structured check_typed"
generator_targets="check_structured check_typed codegen codegen_run lsp_edits"
all_targets="lex parse parse_roundtrip structured check check_structured check_typed codegen codegen_run lsp_edits http_request rpc_json"
check_max=2048

case "${1:-}" in
    --fresh)
        stamp=$(date +%s)
        mkdir -p "$here/corpus-old"
        for target in $semantic_targets; do
            if [ -d "$here/corpus/$target" ]; then
                mv "$here/corpus/$target" "$here/corpus-old/$target.$stamp"
                echo "moved fuzz/corpus/$target to fuzz/corpus-old/$target.$stamp"
            fi
        done
        ;;
    "") ;;
    *)
        echo "usage: $0 [--fresh]" >&2
        exit 2
        ;;
esac

# Copies `$1` (a path below tests/corpus) into the corpus of target `$2`.
seed() {
    rel=${1#"$corpus/"}
    name=$(printf '%s' "$rel" | tr '/' '_')
    cp "$1" "$here/corpus/$2/$name"
}

for target in $all_targets; do
    mkdir -p "$here/corpus/$target"
done

find "$corpus" -name '*.k' -type f | while read -r file; do
    for target in $text_targets; do
        seed "$file" "$target"
    done
done

# `check`: the median-sized single-file case of each sema directory two
# levels down (a project case's files are left out: alone, they are mostly
# unresolved imports), then every regression case.
find "$corpus/sema" -mindepth 2 -maxdepth 2 -type d | sort | while read -r dir; do
    find "$dir" -name '*.k' -type f -size -$((check_max + 1))c | while read -r file; do
        case_dir=$(dirname "$file")
        is_project=false
        while [ "$case_dir" != "$corpus/sema" ]; do
            if [ -f "$case_dir/k.toml" ]; then
                is_project=true
                break
            fi
            case_dir=$(dirname "$case_dir")
        done
        if [ "$is_project" = false ]; then
            printf '%s %s\n' "$(wc -c < "$file" | tr -d ' ')" "$file"
        fi
    done | sort -n | awk '{ lines[NR] = $2 } END { if (NR > 0) print lines[int((NR + 1) / 2)] }' |
        while read -r file; do
            seed "$file" check
        done
done
find "$corpus/sema" "$corpus/parser" "$corpus/lexer" -path '*regressions*' -name '*.k' -type f \
    -size -$((check_max + 1))c | while read -r file; do
    seed "$file" check
done

for target in http_request rpc_json; do
    cp "$here/seeds/$target/"* "$here/corpus/$target/"
done

for target in $generator_targets; do
    case $target in
        check_* | lsp_edits) step=64 ;;
        *) step=256 ;;
    esac
    for n in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16; do
        dd if=/dev/urandom of="$here/corpus/$target/random-$n" bs=$((n * step)) count=1 2>/dev/null
    done
done

empty=""
for target in $all_targets; do
    count=$(find "$here/corpus/$target" -type f | wc -l | tr -d ' ')
    echo "fuzz/corpus/$target: $count inputs"
    if [ "$count" -eq 0 ]; then
        empty="$empty $target"
    fi
done
if [ -n "$empty" ]; then
    echo "error: no seeds for:$empty" >&2
    exit 1
fi
