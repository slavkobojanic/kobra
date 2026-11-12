#!/bin/sh
# Assembles the toolchain index (index.json) from every published release's
# index-entry.json (written by `cargo xtask dist index` in release.yml and
# nightly.yml), for the docs site (docs/compiler/release.md §9). Run by the
# Pages jobs with GH_TOKEN set; needs `gh`.
#
#   scripts/ci/release-index.sh target/site/index.json
set -eu
out="$1"
entries="$(mktemp -d)"
gh release list --exclude-drafts --limit 1000 --json tagName --jq '.[].tagName' > "$entries/tags"
while read -r tag; do
    gh release download "$tag" --pattern index-entry.json --output "$entries/$tag.json" \
        || echo "release-index.sh: $tag has no index-entry.json; skipped" >&2
done < "$entries/tags"
rm "$entries/tags"
cargo xtask dist merge-index "$entries" --out "$out"
