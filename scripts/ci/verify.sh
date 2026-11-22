#!/bin/sh
# Verifies a downloaded release as a user would: every archive's digest
# against SHA256SUMS, and its Sigstore bundle against the identity of the
# workflow that signed it, at the ref it was started on (release.md §9).
# Run by release.yml's `verify` job; needs `cosign`.
#
#   scripts/ci/verify.sh target/dist 'https://github.com/<owner>/<repo>/.github/workflows/release.yml@refs/heads/main'
set -eu
dir="$1"
identity="$2"
cd "$dir"
if command -v sha256sum >/dev/null 2>&1; then
    sha256sum --check --strict SHA256SUMS
else
    shasum -a 256 --check --strict SHA256SUMS
fi
for archive in k-*.tar.gz k-*.zip; do
    [ -f "$archive" ] || continue
    cosign verify-blob --bundle "$archive.sigstore.json" \
        --certificate-identity "$identity" \
        --certificate-oidc-issuer https://token.actions.githubusercontent.com \
        "$archive"
done
