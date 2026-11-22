#!/bin/sh
# Installs the Kobra toolchain, the `k` command
# (docs/design/48-releases-and-versioning.md, "Artifacts";
# docs/compiler/release.md §12 step 11).
#
#   curl -fsSL https://slavkobojanic.github.io/kobra/install.sh | sh
#   curl -fsSL https://slavkobojanic.github.io/kobra/install.sh | sh -s -- --version 0.1.0
#
# It picks the archive for this machine (`uname`), downloads it and the
# release's SHA256SUMS from GitHub Releases, checks the digest, unpacks the
# toolchain into $K_HOME/toolchains/<version>/, makes it the default
# ($K_HOME/bin/k, the version in $K_HOME/default; the layout `k toolchain`
# uses) and prints the line that puts $K_HOME/bin on the PATH. Running it
# again changes nothing. While no stable release exists, it installs the
# newest release candidate (else the newest nightly) and says so.
#
# Variables:
#   K_RELEASES         the release host: a URL with latest/download/<file>
#                      (the newest stable release) and
#                      download/v<version>/<file>. The docs site's copy
#                      has it filled in.
#   K_TOOLCHAIN_INDEX  the toolchain index (index.json), read to find the
#                      newest pre-release when there is no stable release;
#                      the docs site's copy has it filled in
#   K_VERSION          a version to install instead of the newest stable
#                      (also --version <version>)
#   K_HOME             where toolchains go; ~/.k by default
#
# Needs: sh, uname, curl or wget, sha256sum or shasum, tar, mktemp, awk.

set -eu

RELEASES="${K_RELEASES:-@K_RELEASES@}"
INDEX="${K_TOOLCHAIN_INDEX:-@K_TOOLCHAIN_INDEX@}"
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
            sed -n '2,29p' "$0" 2>/dev/null | sed 's/^# \{0,1\}//'
            exit 0
            ;;
        *) fail "unknown argument \`$1\`; see --help" ;;
    esac
done

case "$RELEASES" in
    @*) fail "this copy of install.sh names no release host (none is published yet); set K_RELEASES" ;;
esac
RELEASES="${RELEASES%/}"

# The C library: Kobra's Linux builds link glibc; musl is not supported.
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
        *) fail "Kobra releases no toolchain for the $arch processor; build k from source" ;;
    esac
    case "$os" in
        Darwin) say "$arch-apple-darwin" ;;
        Linux)
            if is_musl; then
                fail "this Linux uses musl, and Kobra's Linux toolchains need glibc (2.31 or newer); build k from source, or use a glibc distribution or container"
            fi
            say "$arch-unknown-linux-gnu"
            ;;
        MINGW* | MSYS* | CYGWIN* | Windows_NT) fail "on Windows, install Kobra with install.ps1 in PowerShell" ;;
        *) fail "Kobra releases no toolchain for $os; build k from source" ;;
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

# newest <index.json> <target>: the newest release in the toolchain index
# that isn't yanked and has an archive for <target>, a stable release
# before a candidate before a nightly, as `<version> <channel>`.
newest() {
    awk -v target="$2" '
    function keep(   core, pre, n, p, rank, num, key) {
        if (version == "" || yanked || !has) return
        core = version; pre = ""
        if (index(version, "-") > 0) {
            core = substr(version, 1, index(version, "-") - 1)
            pre = substr(version, index(version, "-") + 1)
        }
        n = split(core, p, ".")
        if (n != 3) return
        if (pre == "") { rank = 3; num = 0 }
        else if (pre ~ /^rc\.[0-9]+$/) { rank = 2; num = substr(pre, 4) + 0 }
        else if (pre ~ /^nightly\.[0-9]+$/) { rank = 1; num = substr(pre, 9) + 0 }
        else return
        key = sprintf("%d %09d %09d %09d %012d", rank, p[1], p[2], p[3], num)
        if (key > best) { best = key; chosen = version; channel = rank }
    }
    { text = text $0 "\n" }
    END {
        n = length(text); depth = 0; key = ""; best = ""
        for (i = 1; i <= n; i++) {
            c = substr(text, i, 1)
            if (c == "\"") {
                s = ""
                for (j = i + 1; j <= n; j++) {
                    d = substr(text, j, 1)
                    if (d == "\\") { s = s substr(text, j + 1, 1); j++; continue }
                    if (d == "\"") break
                    s = s d
                }
                i = j
                k = i + 1
                while (k <= n && substr(text, k, 1) ~ /[ \t\r\n]/) k++
                if (substr(text, k, 1) == ":") { key = s; i = k; continue }
                # A string value: depth 3 is a release, 5 one of its archives.
                if (depth == 3 && key == "version") version = s
                if (depth == 3 && key == "yanked") yanked = 1
                if (depth == 5 && key == "target" && s == target) has = 1
                key = ""
            } else if (c == "{" || c == "[") {
                depth++
                if (depth == 3) { version = ""; yanked = 0; has = 0 }
                key = ""
            } else if (c == "}" || c == "]") {
                if (depth == 3) keep()
                depth--
                key = ""
            } else if (c == ",") {
                key = ""
            }
        }
        if (chosen != "") print chosen, (channel == 3 ? "stable" : channel == 2 ? "candidate" : "nightly")
    }' "$1"
}

TARGET="$(target)"
mkdir -p "$K_HOME/tmp"
WORK="$(mktemp -d "$K_HOME/tmp/install-sh.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT INT TERM

if [ -n "$VERSION" ]; then
    FROM="$RELEASES/download/v${VERSION#v}"
    fetch "$FROM/SHA256SUMS" "$WORK/SHA256SUMS"
else
    FROM="$RELEASES/latest/download"
    # GitHub's `latest` skips pre-releases: with only release candidates
    # (or nightlies) published there is none, and the index names them.
    if ! (fetch "$FROM/SHA256SUMS" "$WORK/SHA256SUMS") 2>/dev/null; then
        NO_STABLE="there is no stable release at $RELEASES yet; install a pre-release by name with --version <version>"
        case "$INDEX" in
            @* | "") fail "$NO_STABLE" ;;
        esac
        (fetch "$INDEX" "$WORK/index.json") 2>/dev/null || fail "$NO_STABLE"
        CHOSEN="$(newest "$WORK/index.json" "$TARGET")"
        [ -n "$CHOSEN" ] || fail "$NO_STABLE"
        VERSION="${CHOSEN%% *}"
        case "${CHOSEN#* }" in
            candidate) say "note: there is no stable release of Kobra yet; installing the newest release candidate, $VERSION (choose another with --version <version>)" ;;
            nightly) say "note: there is no stable release or release candidate of Kobra yet; installing the newest nightly, $VERSION (choose another with --version <version>)" ;;
        esac
        FROM="$RELEASES/download/v$VERSION"
        fetch "$FROM/SHA256SUMS" "$WORK/SHA256SUMS"
    fi
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
