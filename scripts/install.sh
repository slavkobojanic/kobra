#!/bin/sh
# Installs the Kobra toolchain, the `k` command, for collaborators on the
# private repository slavkobojanic/kobra
# (docs/design/48-releases-and-versioning.md, "Private distribution";
# docs/compiler/release.md §2).
#
#   gh api repos/slavkobojanic/kobra/contents/scripts/install.sh -H 'Accept: application/vnd.github.raw' | sh
#   gh api repos/slavkobojanic/kobra/contents/scripts/install.sh -H 'Accept: application/vnd.github.raw' | sh -s -- --version 0.1.0
#
# The releases are private, so every download is authenticated: with the
# GitHub CLI (`gh`, signed in with `gh auth login`), else with curl and the
# token in GH_TOKEN or GITHUB_TOKEN. It picks the archive for this machine
# (`uname`), downloads it and the release's SHA256SUMS from the GitHub
# release, checks the digest, unpacks the toolchain into
# $K_HOME/toolchains/<version>/, makes it the default ($K_HOME/bin/k, the
# version in $K_HOME/default; the layout `k toolchain` uses), unpacks the
# release's book into $K_HOME/docs/<version>/ (read it with `k docs`) and
# prints the line that puts $K_HOME/bin on the PATH. Running it again
# changes nothing. While no stable release exists, it installs the newest
# release candidate (else the newest nightly) from the newest release's
# index.json and says so.
#
# Variables:
#   K_VERSION          a version to install instead of the newest stable
#                      (also --version <version>)
#   K_HOME             where toolchains go; ~/.k by default
#   GH_TOKEN           a token that can read the repository, for curl when
#                      gh isn't installed (GITHUB_TOKEN also works)
#   K_INSTALL_GH       the gh command to use (`gh`); `none` to use curl
#   K_GITHUB_API       GitHub's API (https://api.github.com), for tests
#
# Needs: sh, uname, gh (or curl and a token), sha256sum or shasum, tar,
# mktemp, awk.

set -eu

REPO="slavkobojanic/kobra"
API="${K_GITHUB_API:-https://api.github.com}"
API="${API%/}"
GH="${K_INSTALL_GH-gh}"
TOKEN="${GH_TOKEN:-${GITHUB_TOKEN:-}}"
VERSION="${K_VERSION:-}"
K_HOME="${K_HOME:-${HOME:?HOME is not set}/.k}"

say() { printf '%s\n' "$*"; }
fail() {
    printf 'error: %s\n' "$*" >&2
    exit 1
}

SIGN_IN="sign in with the GitHub CLI (https://cli.github.com): \`gh auth login\`, or set GH_TOKEN to a token that can read $REPO"

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
            say "install.sh: installs Kobra's \`k\` from the private repository $REPO"
            say "usage: sh install.sh [--version <version>]"
            say "needs gh (signed in with \`gh auth login\`), or curl with GH_TOKEN set"
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

# How to reach the private releases: `gh`, else curl with the token.
if [ -n "$GH" ] && [ "$GH" != none ] && command -v "$GH" >/dev/null 2>&1; then
    MODE=gh
elif [ -n "$TOKEN" ]; then
    command -v curl >/dev/null 2>&1 || fail "with GH_TOKEN and without gh, the install needs curl"
    MODE=curl
else
    fail "Kobra's repository $REPO is private, and neither gh nor a token was found: install the GitHub CLI (https://cli.github.com) and run \`gh auth login\`, or set GH_TOKEN (or GITHUB_TOKEN) to a token that can read $REPO"
fi

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

# api <path> <file>: GET an API path (`repos/<repo>/releases/latest`) into
# <file>; on failure, the error is in $WORK/err.
api() {
    if [ "$MODE" = gh ]; then
        "$GH" api -H 'Accept: application/vnd.github+json' "$1" >"$2" 2>"$WORK/err"
    else
        code="$(curl -sS -L -o "$2" -w '%{http_code}' \
            -H "Authorization: Bearer $TOKEN" \
            -H 'Accept: application/vnd.github+json' \
            -H 'X-GitHub-Api-Version: 2022-11-28' \
            "$API/$1" 2>"$WORK/err")" || code=000
        [ "$code" = 200 ] && return 0
        printf 'GitHub answered HTTP %s for %s\n' "$code" "$API/$1" >>"$WORK/err"
        return 1
    fi
}

# api_failed: stops with what $WORK/err says, explained.
api_failed() {
    err="$(cat "$WORK/err" 2>/dev/null || true)"
    case "$err" in
        *404* | *"Not Found"*) fail "GitHub answered 404: you may not have access to $REPO; $SIGN_IN" ;;
        *401* | *"auth login"* | *"authentication"* | *"Bad credentials"*) fail "GitHub refused the request: you are not signed in; $SIGN_IN" ;;
        *) fail "cannot reach GitHub: $err" ;;
    esac
}

# asset <tag> <name> <file>: downloads a release's asset; non-zero when the
# release or the asset isn't there.
asset() {
    if [ "$MODE" = gh ]; then
        "$GH" release download "$1" -R "$REPO" -p "$2" -O "$3" --clobber >/dev/null 2>"$WORK/err"
        return
    fi
    if [ ! -f "$WORK/release-$1.json" ]; then
        api "repos/$REPO/releases/tags/$1" "$WORK/release-$1.json" || {
            rm -f "$WORK/release-$1.json"
            return 1
        }
    fi
    url="$(flat "$WORK/release-$1.json" | awk -F '\t' -v want="$2" '
        $1 ~ /^\.assets\.[0-9]+\.name$/ && $2 == want { split($1, p, "."); n = p[3] }
        $1 ~ /^\.assets\.[0-9]+\.url$/ { split($1, p, "."); url[p[3]] = $2 }
        END { if (n != "") print url[n] }')"
    [ -n "$url" ] || return 1
    # curl sends the token to the API only: an asset's API URL redirects to
    # a signed storage URL on another host, which doesn't get it.
    curl -fsSL -o "$3" \
        -H "Authorization: Bearer $TOKEN" \
        -H 'Accept: application/octet-stream' \
        "$url" 2>"$WORK/err"
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

# Access first, so a missing permission isn't mistaken for a missing release.
api "repos/$REPO" "$WORK/repo.json" || api_failed

if [ -n "$VERSION" ]; then
    TAG="v${VERSION#v}"
elif api "repos/$REPO/releases/latest" "$WORK/latest.json"; then
    TAG="$(flat "$WORK/latest.json" | awk -F '\t' '$1 == ".tag_name" { print $2; exit }')"
else
    # GitHub's `latest` skips pre-releases: with only release candidates
    # (or nightlies) published there is none, and the newest release's
    # index names them.
    NO_STABLE="there is no stable release of Kobra yet; install a pre-release by name with --version <version>"
    api "repos/$REPO/releases?per_page=100" "$WORK/releases.json" || api_failed
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
    asset "$INDEX_TAG" index.json "$WORK/index.json" || fail "$NO_STABLE"
    CHOSEN="$(newest "$WORK/index.json" "$TARGET")"
    [ -n "$CHOSEN" ] || fail "$NO_STABLE"
    VERSION="${CHOSEN%% *}"
    case "${CHOSEN#* }" in
        candidate) say "note: there is no stable release of Kobra yet; installing the newest release candidate, $VERSION (choose another with --version <version>)" ;;
        nightly) say "note: there is no stable release or release candidate of Kobra yet; installing the newest nightly, $VERSION (choose another with --version <version>)" ;;
    esac
    TAG="v$VERSION"
fi
[ -n "$TAG" ] || fail "GitHub named no newest release of $REPO"
asset "$TAG" SHA256SUMS "$WORK/SHA256SUMS" || fail "cannot download SHA256SUMS of $REPO's release $TAG (is it a released version?)"

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

DEST="$K_HOME/toolchains/$FOUND"
if [ -x "$DEST/k" ]; then
    say "k $FOUND is already installed in $DEST"
else
    asset "$TAG" "$ARCHIVE" "$WORK/$ARCHIVE" || fail "cannot download $ARCHIVE from the release $TAG"
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
    if asset "$TAG" "docs-$FOUND.tar.gz" "$WORK/docs.tar.gz" &&
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
