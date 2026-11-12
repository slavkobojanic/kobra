#!/bin/sh
# Signs every release archive in DIR with Sigstore's keyless signing: a
# `<archive>.sigstore.json` bundle each, from the workflow's OIDC token
# (design 48, "Owner decisions": Sigstore keyless with build provenance).
# Run by release.yml's `sign` job; needs `cosign`.
#
#   scripts/ci/sign.sh target/dist
set -eu
dir="$1"
found=0
for archive in "$dir"/k-*.tar.gz "$dir"/k-*.zip; do
    [ -f "$archive" ] || continue
    cosign sign-blob --yes --bundle "$archive.sigstore.json" "$archive"
    found=1
done
[ "$found" = 1 ] || { echo "sign.sh: no archive in $dir" >&2; exit 1; }
