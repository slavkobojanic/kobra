#!/bin/sh
# Publishes the release of TAG, made as a draft by release.yml, once the
# macOS archive is attached (`cargo xtask release attach-local TAG` on the
# owner's Mac; release.md §6). Without `--no-macos` it refuses a release
# that has no `aarch64-apple-darwin` archive. A release published already
# is left as it is, so publish.yml can be re-run. Run by publish.yml's
# `publish` job with GH_TOKEN set; needs `gh`.
#
#   scripts/ci/publish-release.sh v0.1.0 [--no-macos]
set -eu
tag="$1"
macos="${2:-}"
assets="$(gh release view "$tag" --json assets --jq '.assets[].name')"
case "$assets" in
    *-aarch64-apple-darwin.tar.gz*) ;;
    *)
        if [ "$macos" != "--no-macos" ]; then
            echo "publish-release.sh: $tag has no macOS archive: run \`cargo xtask release attach-local $tag\` on the Mac first, or publish with \`-f macos=false\`" >&2
            exit 1
        fi
        ;;
esac
if [ "$(gh release view "$tag" --json isDraft --jq '.isDraft')" = "true" ]; then
    gh release edit "$tag" --draft=false
    echo "publish-release.sh: published $tag"
else
    echo "publish-release.sh: $tag is published already"
fi
