#!/bin/sh
# Installs the Kobra toolchain, the `k` command
# (docs/design/48-releases-and-versioning.md, "Public distribution";
# docs/compiler/release.md §2). The docs site serves this file:
#
#   curl -fsSL https://slavkobojanic.github.io/kobra/install.sh | sh
#   curl -fsSL https://slavkobojanic.github.io/kobra/install.sh | sh -s -- --version 0.1.0
#
# It picks the archive for this machine (`uname`), downloads it and the
# release's SHA256SUMS from the public GitHub Releases of
# slavkobojanic/kobra, checks the digest, unpacks the toolchain into
# $K_HOME/toolchains/<version>/, makes it the default ($K_HOME/bin/k, the
# version in $K_HOME/default; the layout `k toolchain` uses), unpacks the
# release's book into $K_HOME/docs/<version>/ (read it with `k docs`) and
# prints the line that puts $K_HOME/bin on the PATH. Running it again
# changes nothing. While no stable release exists, it installs the newest
# release candidate (else the newest nightly) from the docs site's
# index.json (else the newest release's) and says so.
#
# No sign-in is needed. Only the fallback for a site without index.json
# asks GitHub's API (which allows 60 requests an hour without a token):
# there a token is sent when one is found, in GH_TOKEN, GITHUB_TOKEN or
# `gh auth token`, and only to the API.
#
# Variables:
#   K_VERSION          a version to install instead of the newest stable
#                      (also --version <version>)
#   K_HOME             where toolchains go; ~/.k by default
#   GH_TOKEN           optional: a token for GitHub's API (GITHUB_TOKEN
#                      also works; else `gh auth token` when gh is installed)
#   K_RELEASES         the releases' download URLs, for tests:
#                      https://github.com/slavkobojanic/kobra/releases,
#                      with latest/download/<file> and download/v<version>/<file>
#   K_TOOLCHAIN_INDEX  the toolchain index, for tests:
#                      https://slavkobojanic.github.io/kobra/index.json
#   K_GITHUB_API       GitHub's API (https://api.github.com), for tests
#   K_INSTALL_GH       the gh command to ask for a token (`gh`); `none` for none
#
# Needs: sh, uname, curl or wget, sha256sum or shasum, tar, mktemp, awk.

set -eu

REPO="slavkobojanic/kobra"
RELEASES="${K_RELEASES:-https://github.com/slavkobojanic/kobra/releases}"
RELEASES="${RELEASES%/}"
INDEX="${K_TOOLCHAIN_INDEX:-https://slavkobojanic.github.io/kobra/index.json}"
API="${K_GITHUB_API:-https://api.github.com}"
API="${API%/}"
GH="${K_INSTALL_GH-gh}"
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
            say "install.sh: installs Kobra's \`k\` from the GitHub Releases of $REPO"
            say "usage: sh install.sh [--version <version>]"
            say "variables: K_VERSION, K_HOME; GH_TOKEN (optional, for GitHub's API)"
            exit 0
            ;;
        *) fail "unknown argument \`$1\`; see --help" ;;
    esac
done

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

# flat <file.json>: one `<path>\t<value>` line per value of a JSON
# document, the path's parts joined with `.` (`.0.assets.1.name`), for the
# few fields read below (no jq needed).
flat() {
    awk '
    function emit(v,   p, k) {
        p = ""
        for (k = 1; k < d; k++) p = p "." seg[k]
        p = p "." (type[d] == "a" ? idx[d] : key[d])
        print p "\t" v
    }
    { text = text $0 "\n" }
    END {
        n = length(text); d = 0
        for (i = 1; i <= n; i++) {
            c = substr(text, i, 1)
            if (c == " " || c == "\t" || c == "\r" || c == "\n" || c == ":") continue
            if (c == ",") { if (type[d] == "a") idx[d]++; else want[d] = 1; continue }
            if (c == "{" || c == "[") {
                seg[d] = (type[d] == "a" ? idx[d] : key[d])
                d++; type[d] = (c == "{" ? "o" : "a"); idx[d] = 0; key[d] = ""; want[d] = (c == "{")
                continue
            }
            if (c == "}" || c == "]") { d--; continue }
            if (c == "\"") {
                s = ""
                for (j = i + 1; j <= n; j++) {
                    e = substr(text, j, 1)
                    if (e == "\\") { s = s substr(text, j + 1, 1); j++; continue }
                    if (e == "\"") break
                    s = s e
                }
                i = j
                if (type[d] == "o" && want[d]) { key[d] = s; want[d] = 0; continue }
                emit(s)
                continue
            }
            for (j = i; j <= n; j++) if (substr(text, j, 1) ~ /[ \t\r\n,}\]]/) break
            emit(substr(text, i, j - i))
            i = j - 1
        }
    }' "$1"
}

# fetch <url> <file>: a plain download (no token: the files are public);
# non-zero when it fails.
fetch() {
    if command -v curl >/dev/null 2>&1; then
        curl -fsSL --retry 2 -o "$2" "$1" 2>/dev/null
    elif command -v wget >/dev/null 2>&1; then
        wget -q -O "$2" "$1"
    else
        fail "neither curl nor wget is installed"
    fi
}

# token: the optional token for GitHub's API, from GH_TOKEN, GITHUB_TOKEN
# or `gh auth token`; empty without one.
token() {
    if [ -n "${GH_TOKEN:-}" ]; then
        printf '%s' "$GH_TOKEN"
    elif [ -n "${GITHUB_TOKEN:-}" ]; then
        printf '%s' "$GITHUB_TOKEN"
    elif [ -n "$GH" ] && [ "$GH" != none ] && command -v "$GH" >/dev/null 2>&1; then
        "$GH" auth token 2>/dev/null || true
    fi
}

# api <path> <file>: GET an API path into <file>, with the token when there
# is one (sent to the API only); non-zero when it fails.
api() {
    tok="$(token)"
    if command -v curl >/dev/null 2>&1; then
        if [ -n "$tok" ]; then
            curl -fsSL -o "$2" -H "Authorization: Bearer $tok" \
                -H 'Accept: application/vnd.github+json' "$API/$1" 2>/dev/null
        else
            curl -fsSL -o "$2" -H 'Accept: application/vnd.github+json' "$API/$1" 2>/dev/null
        fi
    elif command -v wget >/dev/null 2>&1; then
        if [ -n "$tok" ]; then
            wget -q -O "$2" --header "Authorization: Bearer $tok" \
                --header 'Accept: application/vnd.github+json' "$API/$1"
        else
            wget -q -O "$2" --header 'Accept: application/vnd.github+json' "$API/$1"
        fi
    else
        fail "neither curl nor wget is installed"
    fi
}

# newest <index.json> <target>: the newest release in the toolchain index
# that isn't yanked and has an archive for <target>, a stable release
# before a candidate before a nightly, as `<version> <channel>`.
newest() {
    flat "$1" | awk -F '\t' -v target="$2" '
    $1 ~ /^\.releases\.[0-9]+\.version$/ { split($1, p, "."); version[p[3]] = $2; seen[p[3]] = 1 }
    $1 ~ /^\.releases\.[0-9]+\.yanked$/ { split($1, p, "."); yanked[p[3]] = 1 }
    $1 ~ /^\.releases\.[0-9]+\.archives\.[0-9]+\.target$/ && $2 == target { split($1, p, "."); has[p[3]] = 1 }
    END {
        best = ""
        for (r in seen) {
            v = version[r]
            if (v == "" || yanked[r] || !has[r]) continue
            core = v; pre = ""
            if (index(v, "-") > 0) { core = substr(v, 1, index(v, "-") - 1); pre = substr(v, index(v, "-") + 1) }
            if (split(core, q, ".") != 3) continue
            if (pre == "") { rank = 3; num = 0 }
            else if (pre ~ /^rc\.[0-9]+$/) { rank = 2; num = substr(pre, 4) + 0 }
            else if (pre ~ /^nightly\.[0-9]+$/) { rank = 1; num = substr(pre, 9) + 0 }
            else continue
            key = sprintf("%d %09d %09d %09d %012d", rank, q[1], q[2], q[3], num)
            if (key > best) { best = key; chosen = v; channel = rank }
        }
        if (chosen != "") print chosen, (channel == 3 ? "stable" : channel == 2 ? "candidate" : "nightly")
    }'
}

TARGET="$(target)"
mkdir -p "$K_HOME/tmp"
WORK="$(mktemp -d "$K_HOME/tmp/install-sh.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT INT TERM

if [ -n "$VERSION" ]; then
    TAG="v${VERSION#v}"
    fetch "$RELEASES/download/$TAG/SHA256SUMS" "$WORK/SHA256SUMS" ||
        fail "cannot download SHA256SUMS of the release $TAG from $RELEASES (is it a released version?)"
elif ! fetch "$RELEASES/latest/download/SHA256SUMS" "$WORK/SHA256SUMS"; then
    # GitHub's `latest` skips pre-releases: with only release candidates
    # (or nightlies) published there is none, and the toolchain index
    # names them: the docs site's, else the newest release's.
    NO_STABLE="there is no stable release of Kobra yet; install a pre-release by name with --version <version>"
    if ! fetch "$INDEX" "$WORK/index.json"; then
        api "repos/$REPO/releases?per_page=100" "$WORK/releases.json" ||
            fail "$NO_STABLE (GitHub's API didn't answer; set GH_TOKEN if it limited the requests)"
        INDEX_TAG="$(flat "$WORK/releases.json" | awk -F '\t' '
            $1 ~ /^\.[0-9]+\.tag_name$/ { split($1, p, "."); tag[p[2]] = $2 }
            $1 ~ /^\.[0-9]+\.draft$/ { split($1, p, "."); draft[p[2]] = $2 }
            $1 ~ /^\.[0-9]+\.published_at$/ { split($1, p, "."); at[p[2]] = $2 }
            $1 ~ /^\.[0-9]+\.assets\.[0-9]+\.name$/ && $2 == "index.json" { split($1, p, "."); has[p[2]] = 1 }
            END {
                for (r in tag) if (has[r] && draft[r] != "true" && at[r] != "null" && at[r] > best) { best = at[r]; chosen = tag[r] }
                if (chosen != "") print chosen
            }')"
        [ -n "$INDEX_TAG" ] || fail "$NO_STABLE"
        fetch "$RELEASES/download/$INDEX_TAG/index.json" "$WORK/index.json" || fail "$NO_STABLE"
    fi
    CHOSEN="$(newest "$WORK/index.json" "$TARGET")"
    [ -n "$CHOSEN" ] || fail "$NO_STABLE"
    VERSION="${CHOSEN%% *}"
    case "${CHOSEN#* }" in
        candidate) say "note: there is no stable release of Kobra yet; installing the newest release candidate, $VERSION (choose another with --version <version>)" ;;
        nightly) say "note: there is no stable release or release candidate of Kobra yet; installing the newest nightly, $VERSION (choose another with --version <version>)" ;;
    esac
    TAG="v$VERSION"
    fetch "$RELEASES/download/$TAG/SHA256SUMS" "$WORK/SHA256SUMS" ||
        fail "cannot download SHA256SUMS of the release $TAG from $RELEASES"
fi

# `<digest>  k-<version>-<target>.tar.gz` (or `*name`, binary mode).
LINE="$(awk -v suffix="-$TARGET.tar.gz" '{
    name = $2; sub(/^\*/, "", name)
    if (substr(name, 1, 2) == "k-" && length(name) > length(suffix) &&
        substr(name, length(name) - length(suffix) + 1) == suffix) { print $1, name; exit }
}' "$WORK/SHA256SUMS")"
[ -n "$LINE" ] || fail "the release $TAG has no archive for $TARGET"
DIGEST="${LINE%% *}"
ARCHIVE="${LINE#* }"
FOUND="${ARCHIVE#k-}"
FOUND="${FOUND%-"$TARGET".tar.gz}"
# The files of that release (`latest/download/` may move meanwhile).
TAG="v$FOUND"
FROM="$RELEASES/download/$TAG"

DEST="$K_HOME/toolchains/$FOUND"
if [ -x "$DEST/k" ]; then
    say "k $FOUND is already installed in $DEST"
else
    fetch "$FROM/$ARCHIVE" "$WORK/$ARCHIVE" || fail "cannot download $ARCHIVE from the release $TAG ($FROM)"
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

# The book, for `k docs`: optional, so a failure is only a warning.
BOOK="$K_HOME/docs/$FOUND"
DOCS_LINE="$(awk -v want="docs-$FOUND.tar.gz" '{ name = $2; sub(/^\*/, "", name); if (name == want) { print $1; exit } }' "$WORK/SHA256SUMS")"
if [ -n "$DOCS_LINE" ] && [ ! -f "$BOOK/index.html" ]; then
    if fetch "$FROM/docs-$FOUND.tar.gz" "$WORK/docs.tar.gz" &&
        [ "$(sha256 "$WORK/docs.tar.gz")" = "$DOCS_LINE" ] &&
        mkdir -p "$WORK/book" && tar -xzf "$WORK/docs.tar.gz" -C "$WORK/book" &&
        [ -f "$WORK/book/docs-$FOUND/index.html" ]; then
        mkdir -p "$K_HOME/docs"
        rm -rf "$BOOK"
        mv "$WORK/book/docs-$FOUND" "$BOOK"
        say "installed the book of k $FOUND in $BOOK (open it with \`k docs\`)"
    else
        say "warning: the book of k $FOUND wasn't installed (its download or checksum failed)"
    fi
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
