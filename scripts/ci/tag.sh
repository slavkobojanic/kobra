#!/bin/sh
# Creates the annotated release tag on the commit the release workflow
# gated, and pushes it (docs/compiler/release.md §9, "release.yml": its
# `release` job, the only one that may write the repository). A re-run of
# that job finds the tag already on the commit and leaves it; a tag on
# another commit is refused, since a tag is never moved or reused
# (release.md §5). Needs a checkout with the tags (fetch-depth: 0) whose
# `origin` accepts the push.
#
#   scripts/ci/tag.sh v0.1.0 <commit>
set -eu
tag="$1"
sha="$2"
if git rev-parse -q --verify "refs/tags/$tag" >/dev/null; then
    at="$(git rev-parse "$tag^{commit}")"
    if [ "$at" = "$sha" ]; then
        echo "tag.sh: $tag is on $sha already; left as it is"
        exit 0
    fi
    echo "tag.sh: $tag is on $at, not $sha: a tag is never moved or reused (docs/compiler/release.md §5)" >&2
    exit 1
fi
git -c user.name="github-actions[bot]" \
    -c user.email="41898282+github-actions[bot]@users.noreply.github.com" \
    tag -a "$tag" -m "Kobra ${tag#v}" "$sha"
git push origin "refs/tags/$tag"
