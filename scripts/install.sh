#!/bin/sh
# Installs the K toolchain (docs/design/48-releases-and-versioning.md,
# "Artifacts"; docs/compiler/release.md §12 step 11).
#
#   curl -fsSL <docs site>/install.sh | sh
#   curl -fsSL <docs site>/install.sh | sh -s -- --version 0.1.0
#
# It picks the archive for this machine (`uname`), downloads it and the
# release's SHA256SUMS, checks the digest, unpacks the toolchain into
# $K_HOME/toolchains/<version>/, makes it the default ($K_HOME/bin/k, the
# version in $K_HOME/default; the layout `k toolchain` uses) and prints the
# line that puts $K_HOME/bin on the PATH. Running it again changes nothing.
#
# Variables:
#   K_RELEASES  the release host: a URL with latest/download/<file> (the
#               newest stable release) and download/v<version>/<file>.
#               The docs site's copy has it filled in.
#   K_VERSION   a version to install instead of the newest stable (also
#               --version <version>)
#   K_HOME      where toolchains go; ~/.k by default
#
# Needs: sh, uname, curl or wget, sha256sum or shasum, tar, mktemp.

set -eu

RELEASES="${K_RELEASES:-@K_RELEASES@}"
VERSION="${K_VERSION:-}"
K_HOME="${K_HOME:-${HOME:?HOME is not set}/.k}"

say() { printf '%s\n' "$*"; }
fail() {
    printf 'error: %s\n' "$*" >&2
    exit 1
}

while [ $# -gt 0 ]; do
    case "$1" in
        --version)
            [ $# -ge 2 ] || fail "--version needs a version, such as 0.1.0"
            VERSION="$2"
            shift 2
            ;;
        --version=*)
            VERSION="${1#--version=}"
            shift
            ;;
        -h | --help)
            sed -n '2,24p' "$0" 2>/dev/null | sed 's/^# \{0,1\}//'
            exit 0
            ;;
        *) fail "unknown argument \`$1\`; see --help" ;;
    esac
done

case "$RELEASES" in
    @*) fail "this copy of install.sh names no release host (none is published yet); set K_RELEASES" ;;
esac
RELEASES="${RELEASES%/}"

# The C library: K's Linux builds link glibc; musl is not supported.
is_musl() {
    for f in /lib/ld-musl-*; do
        [ -e "$f" ] && return 0
    done
    (ldd --version 2>&1 || true) | grep -qi musl
}

# The release target of this machine.
target() {
    os="$(uname -s)"
    arch="$(uname -m)"
    case "$arch" in
        x86_64 | amd64) arch=x86_64 ;;
        arm64 | aarch64) arch=aarch64 ;;
        *) fail "K releases no toolchain for the $arch processor; build k from source" ;;
    esac
    case "$os" in
        Darwin) say "$arch-apple-darwin" ;;
        Linux)
            if is_musl; then
                fail "this Linux uses musl, and K's Linux toolchains need glibc (2.31 or newer); build k from source, or use a glibc distribution or container"
            fi
            say "$arch-unknown-linux-gnu"
            ;;
        MINGW* | MSYS* | CYGWIN* | Windows_NT) fail "on Windows, install K with install.ps1 in PowerShell" ;;
        *) fail "K releases no toolchain for $os; build k from source" ;;
    esac
}

# fetch <url> <file>
fetch() {
    if command -v curl >/dev/null 2>&1; then
        curl -fsSL --retry 2 -o "$2" "$1" || fail "cannot download $1"
    elif command -v wget >/dev/null 2>&1; then
        wget -q -O "$2" "$1" || fail "cannot download $1"
    else
        fail "neither curl nor wget is installed"
    fi
}

# sha256 <file>
sha256() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | cut -d ' ' -f 1
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$1" | cut -d ' ' -f 1
    else
        fail "neither sha256sum nor shasum is installed"
    fi
}

TARGET="$(target)"
if [ -n "$VERSION" ]; then
    FROM="$RELEASES/download/v${VERSION#v}"
else
    FROM="$RELEASES/latest/download"
fi

mkdir -p "$K_HOME/tmp"
WORK="$(mktemp -d "$K_HOME/tmp/install-sh.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT INT TERM

if [ -n "$VERSION" ]; then
    fetch "$FROM/SHA256SUMS" "$WORK/SHA256SUMS"
else
    # GitHub's `latest` skips pre-releases: with only release candidates
    # published there is none.
    (fetch "$FROM/SHA256SUMS" "$WORK/SHA256SUMS") ||
        fail "there is no stable release at $RELEASES yet; install a release candidate by name with --version <version>"
fi
# `<digest>  k-<version>-<target>.tar.gz` (or `*name`, binary mode).
LINE="$(awk -v suffix="-$TARGET.tar.gz" '{
    name = $2; sub(/^\*/, "", name)
    if (substr(name, 1, 2) == "k-" && length(name) > length(suffix) &&
        substr(name, length(name) - length(suffix) + 1) == suffix) { print $1, name; exit }
}' "$WORK/SHA256SUMS")"
[ -n "$LINE" ] || fail "the release at $FROM has no archive for $TARGET"
DIGEST="${LINE%% *}"
ARCHIVE="${LINE#* }"
FOUND="${ARCHIVE#k-}"
FOUND="${FOUND%-"$TARGET".tar.gz}"

DEST="$K_HOME/toolchains/$FOUND"
if [ -x "$DEST/k" ]; then
    say "k $FOUND is already installed in $DEST"
else
    fetch "$FROM/$ARCHIVE" "$WORK/$ARCHIVE"
    GOT="$(sha256 "$WORK/$ARCHIVE")"
    [ "$GOT" = "$DIGEST" ] || fail "checksum mismatch for $ARCHIVE: SHA256SUMS says $DIGEST, the download is $GOT; nothing was installed"
    mkdir -p "$WORK/unpacked"
    tar -xzf "$WORK/$ARCHIVE" -C "$WORK/unpacked" || fail "cannot unpack $ARCHIVE"
    # The files are at the archive's root or in its one directory.
    SRC="$WORK/unpacked"
    if [ ! -f "$SRC/k" ]; then
        SRC="$WORK/unpacked/k-$FOUND-$TARGET"
    fi
    [ -f "$SRC/k" ] || fail "$ARCHIVE holds no k; nothing was installed"
    chmod 755 "$SRC/k"
    mkdir -p "$K_HOME/toolchains"
    rm -rf "$DEST"
    mv "$SRC" "$DEST"
    say "installed k $FOUND ($TARGET) in $DEST"
fi

# The default: a copy in bin/, replaced in one rename.
mkdir -p "$K_HOME/bin"
cp "$DEST/k" "$K_HOME/bin/.k.$$"
chmod 755 "$K_HOME/bin/.k.$$"
mv -f "$K_HOME/bin/.k.$$" "$K_HOME/bin/k"
printf '%s\n' "$FOUND" >"$K_HOME/default.$$"
mv -f "$K_HOME/default.$$" "$K_HOME/default"
say "$K_HOME/bin/k is k $FOUND"

case ":$PATH:" in
    *":$K_HOME/bin:"*) ;;
    *)
        say "add k to your PATH: put this line in your shell's profile (~/.profile, ~/.bashrc or ~/.zshrc):"
        say ""
        say "    export PATH=\"$K_HOME/bin:\$PATH\""
        ;;
esac
