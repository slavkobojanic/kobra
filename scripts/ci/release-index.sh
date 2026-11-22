#!/bin/sh
# Assembles the toolchain index (index.json) from every published release's
# index-entry.json (written by `cargo xtask dist index` in release.yml and
# nightly.yml), plus the entries given (the release being made, which isn't
# published yet). Every release and nightly attaches the result: `k
# toolchain` and the install scripts read the newest release's index.json
# (docs/design/48-releases-and-versioning.md, "Private distribution";
# docs/compiler/release.md §9). Run with GH_TOKEN set; needs `gh`.
#
#   scripts/ci/release-index.sh target/dist/index.json [target/dist/index-entry.json ...]
set -eu
out="$1"
shift
entries="$(mktemp -d)"
gh release list --exclude-drafts --limit 1000 --json tagName --jq '.[].tagName' > "$entries/tags"
while read -r tag; do
    gh release download "$tag" --pattern index-entry.json --output "$entries/$tag.json" \
        || echo "release-index.sh: $tag has no index-entry.json; skipped" >&2
done < "$entries/tags"
rm "$entries/tags"
# The given entries sort last (`~`), so they replace a published one of
# the same version.
n=0
for entry in "$@"; do
    n=$((n + 1))
    cp "$entry" "$entries/~$n.json"
done
cargo xtask dist merge-index "$entries" --out "$out"
