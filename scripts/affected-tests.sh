#!/usr/bin/env bash
# Runs only the tests a change can break (docs/compiler/testing.md,
# "Affected tests").
#
# Usage:
#   scripts/affected-tests.sh              # changes since HEAD, working tree and untracked files included
#   scripts/affected-tests.sh main         # changes since the merge base with main
#   scripts/affected-tests.sh --dry-run    # print the choice, run nothing
#   QUICK=1 scripts/affected-tests.sh      # fewer property-test cases (see scripts/test.sh)
#   scripts/affected-tests.sh --files "crates/k_sema/src/lib.rs examples/notes/k.toml"
#                                          # as if exactly these files had changed
#
# How the tests are chosen, for each changed file:
#
#   Cargo.toml, Cargo.lock, .cargo/, .config/, rust-toolchain*, clippy.toml
#       -> everything (scripts/test.sh).
#   crates/<c>/src/**, crates/<c>/Cargo.toml, crates/<c>/build.rs, crates/<c>/js/**
#       -> every test of <c> and of every crate that depends on it, directly
#          or through others (`cargo metadata`; a dev-dependency adds the
#          dependent's tests but does not propagate further), plus their
#          doc tests.
#   crates/k_runtime/js/tests/**
#       -> k_server's runtime_js suite and k_runtime's tests (the JS test
#          files are read from disk, not compiled into the runtime).
#   crates/<c>/tests/<name>.rs (a module of <c>'s test binary `it`, or the
#   root of a binary of its own such as tests/corpus.rs)
#       -> that module's tests (`binary_id(=<c>::it) & test(/^<name>::/)`),
#          or that test binary. tests/main.rs -> all of <c>::it.
#   crates/<c>/tests/**/*.rs (support modules)
#       -> the test modules, in any crate, that use them: through a
#          `#[path = …]` include or `crate::<module>` / `super::<module>`.
#   crates/<c>/tests/** (fixtures, golden files, unused support)
#       -> every test binary of <c>.
#   examples/**, docs/book/**, docs/**, tests/<dir>/**, editors/**,
#   changes/**, CHANGELOG.md, scripts/**
#       -> the test modules (and unit tests) whose source has a string
#          literal naming that directory: "examples", "book", "docs",
#          "<dir>" (e.g. "corpus", "e2e"), "editors", "changes" (the
#          changelog fragments), "CHANGELOG.md", "scripts" (the install
#          scripts' tests). The literal scan is what keeps the choice
#          current as tests are added.
#   anything else (README.md, fuzz/)
#       -> nothing; reported as such.
#
# The script prints what it chose and why, then calls scripts/test.sh with
# the packages, a nextest filter, the in-process suites and the doc-test
# packages it chose.

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT" || exit 2
if [ -f "$HOME/.cargo/env" ]; then
    # shellcheck disable=SC1091
    . "$HOME/.cargo/env"
fi

BASE=HEAD
DRY=0
FILES=""
while [ $# -gt 0 ]; do
    case "$1" in
        --dry-run|-n) DRY=1 ;;
        --files) FILES="$2"; shift ;;
        -h|--help) sed -n '2,44p' "$0"; exit 0 ;;
        -*) echo "scripts/affected-tests.sh: unknown option $1" >&2; exit 2 ;;
        *) BASE="$1" ;;
    esac
    shift
done

if [ "$BASE" != HEAD ]; then
    # A branch name means "since where this branch left it".
    MB="$(git merge-base "$BASE" HEAD 2>/dev/null)" || {
        echo "scripts/affected-tests.sh: unknown git base $BASE" >&2
        exit 2
    }
    BASE="$MB"
fi

if [ -n "$FILES" ]; then
    CHANGED="$(printf '%s\n' $FILES | sort -u)"
else
    CHANGED="$( { git diff --name-only "$BASE" -- ; git ls-files --others --exclude-standard; } | sort -u)"
fi
if [ -z "$CHANGED" ]; then
    echo "No changes since $BASE: nothing to test."
    exit 0
fi

METADATA="$(mktemp "${TMPDIR:-/tmp}/k-affected.XXXXXX")"
trap 'rm -f "$METADATA" "$METADATA.plan" "$METADATA.changed"' EXIT
printf '%s\n' "$CHANGED" >"$METADATA.changed"
cargo metadata --no-deps --format-version 1 --offline >"$METADATA" 2>/dev/null \
    || cargo metadata --no-deps --format-version 1 >"$METADATA" || exit 2

python3 - "$ROOT" "$METADATA" "$METADATA.changed" >"$METADATA.plan" <<'PY'
import json, os, re, sys
from collections import defaultdict

root, meta_path = sys.argv[1], sys.argv[2]
changed = [l.strip() for l in open(sys.argv[3]) if l.strip()]
meta = json.load(open(meta_path))

members = {p["id"] for p in meta["packages"]}
pkgs = {}            # name -> package
pkg_dir = {}         # name -> path relative to root
for p in meta["packages"]:
    if p["id"] not in members:
        continue
    pkgs[p["name"]] = p
    pkg_dir[p["name"]] = os.path.relpath(os.path.dirname(p["manifest_path"]), root)

# Reverse dependencies inside the workspace.
normal_rdeps = defaultdict(set)   # dep -> crates with a normal/build dependency on it
dev_rdeps = defaultdict(set)      # dep -> crates with a dev-dependency on it
for name, p in pkgs.items():
    for d in p["dependencies"]:
        if d["name"] in pkgs:
            (dev_rdeps if d.get("kind") == "dev" else normal_rdeps)[d["name"]].add(name)

def dependents(name):
    """`name` and every workspace crate whose tests can see a change to it."""
    seen, todo = {name}, [name]
    while todo:
        for r in normal_rdeps[todo.pop()]:
            if r not in seen:
                seen.add(r)
                todo.append(r)
    for n in list(seen):
        seen |= dev_rdeps[n]
    return seen

# Test targets: binary id -> (package, kind, root file, files it includes).
def nextest_id(pkg, t):
    kind = t["kind"][0]
    if kind in ("lib", "proc-macro"):
        return pkg
    if kind == "bin":
        return f"{pkg}::bin/{t['name']}"
    return f"{pkg}::{t['name']}"

STRING = re.compile(r'"((?:[^"\\\n]|\\.)*)"')
PATH_ATTR = re.compile(r'#\[path\s*=\s*"([^"]+)"\]')

def code_lines(path):
    try:
        text = open(path, encoding="utf-8", errors="replace").read()
    except OSError:
        return []
    return [l for l in text.splitlines() if not l.lstrip().startswith("//")]

def literals(lines):
    out = []
    for line in lines:
        out.extend(STRING.findall(line))
    return out

def includes(path, depth=0):
    """Files a test root pulls in with #[path = ...] (recursively)."""
    found = []
    if depth > 4:
        return found
    try:
        text = open(path, encoding="utf-8", errors="replace").read()
    except OSError:
        return found
    for rel in PATH_ATTR.findall(text):
        f = os.path.normpath(os.path.join(os.path.dirname(path), rel))
        found.append(f)
        found.extend(includes(f, depth + 1))
    return found

MOD_DECL = re.compile(r'((?:#\[[^\]\n]*\]\s*)*)\bmod\s+(\w+)\s*;')
CRATE_REF = re.compile(r'\b(?:crate|super)::(\w+)')

def scan(files):
    lits, code = set(), []
    for f in files:
        lines = code_lines(f)
        code.extend(lines)
        lits.update(literals(lines))
    return lits, "\n".join(code)

def modules_of(main):
    """A test binary root (tests/main.rs): its test modules, as name -> file,
    and its support modules (declared with #[path]), as name -> file."""
    tests, support = {}, {}
    text = "\n".join(code_lines(main))
    d = os.path.dirname(main)
    for attrs, name in MOD_DECL.findall(text):
        m = PATH_ATTR.search(attrs)
        if m:
            f = os.path.normpath(os.path.join(d, m.group(1)))
            # A directory-rooted former test binary (tests/<name>/main.rs) is a
            # test module; anything else declared with #[path] is support.
            (tests if f == os.path.join(d, name, "main.rs") else support)[name] = f
        else:
            for f in (os.path.join(d, name + ".rs"), os.path.join(d, name, "mod.rs")):
                if os.path.isfile(f):
                    (support if name == "common" else tests)[name] = f
                    break
    return tests, support

def with_support(files, support):
    """`files` plus the support modules they reach through `crate::name` or
    `super::name` (support modules reach each other with `super::`), with
    whatever those include with #[path]."""
    out, todo = set(files), list(files)
    while todo:
        f = todo.pop()
        for name in CRATE_REF.findall("\n".join(code_lines(f))):
            s = support.get(name)
            if s and s not in out:
                out.add(s)
                todo.append(s)
                for inc in includes(s):
                    if inc not in out:
                        out.add(inc)
                        todo.append(inc)
    return out

# A target is a test binary, a lib or bin's unit tests, or one module of a
# crate's `it` binary (docs/design/13-tooling.md, "One test binary per
# crate"): `<pkg>::it/<module>`, selected with a test-name prefix.
targets = {}
for name, p in pkgs.items():
    for t in p["targets"]:
        kind = t["kind"][0]
        if kind not in ("lib", "bin", "test", "proc-macro"):
            continue
        src = t["src_path"]
        if kind == "test" and t["name"] == "it":
            tests, support = modules_of(src)
            for mod, f in tests.items():
                files = with_support([f] + includes(f), support)
                lits, code = scan(files)
                targets[f"{name}::it/{mod}"] = dict(
                    pkg=name, kind=kind, root=os.path.relpath(f, root),
                    files={os.path.relpath(x, root) for x in files}, lits=lits, code=code,
                    binary=f"{name}::it", module=mod)
            continue
        files = [src] + includes(src)
        if kind in ("lib", "bin", "proc-macro"):
            # Unit tests live anywhere under src/.
            src_dir = os.path.join(os.path.dirname(p["manifest_path"]), "src")
            files = [os.path.join(d, f) for d, _, fs in os.walk(src_dir) for f in fs if f.endswith(".rs")]
        rel = {os.path.relpath(f, root) for f in files}
        lits, code = scan(files)
        targets[nextest_id(name, t)] = dict(
            pkg=name, kind=kind, root=os.path.relpath(src, root), files=rel, lits=lits,
            code=code, binary=nextest_id(name, t), module=None)

# Directories reached through a helper rather than a literal path.
HELPERS = {"corpus": re.compile(r"\bcorpus_root\s*\(")}

def consumers(word):
    """Targets with a string literal naming the directory `word` as a path part
    (or calling the helper that locates it)."""
    pat = re.compile(r'(^|[/\\.])' + re.escape(word) + r'($|/)')
    helper = HELPERS.get(word)
    return {
        tid for tid, t in targets.items()
        if any(pat.search(l) for l in t["lits"]) or (helper and helper.search(t["code"]))
    }

IN_PROCESS = {"k_corpus::corpus", "k_server::runtime_js", "k_server::corpus_run"}
WHOLE = ("Cargo.toml", "Cargo.lock", "clippy.toml", "rustfmt.toml")

full = False
whole_pkgs = set()       # every test of these packages (+ doc tests)
some = set()             # single test binaries
reasons = []

def pkg_of(path):
    best = None
    for name, d in pkg_dir.items():
        if path == d or path.startswith(d + "/"):
            if best is None or len(d) > len(pkg_dir[best]):
                best = name
    return best

for f in changed:
    if f in WHOLE or f.startswith((".cargo/", ".config/", "rust-toolchain")):
        full = True
        reasons.append(f"{f}: workspace configuration -> everything")
        continue
    pkg = pkg_of(f)
    if pkg:
        rel = f[len(pkg_dir[pkg]) + 1:]
        if pkg == "k_runtime" and rel.startswith("js/tests/"):
            some.add("k_server::runtime_js")
            whole_pkgs.add("k_runtime")
            reasons.append(f"{f}: a runtime JS test file -> k_server::runtime_js, k_runtime")
        elif rel.startswith("tests/"):
            roots = [tid for tid, t in targets.items() if t["pkg"] == pkg and t["root"] == f]
            users = sorted(tid for tid, t in targets.items() if f in t["files"] and t["root"] != f)
            if roots:
                some.update(roots)
                reasons.append(f"{f}: test root -> {', '.join(roots)}")
            elif f == os.path.join(pkg_dir[pkg], "tests", "main.rs"):
                own = sorted(tid for tid, t in targets.items() if t["binary"] == f"{pkg}::it")
                some.update(own)
                reasons.append(f"{f}: the root of {pkg}::it -> all of {pkg}::it")
            elif users and f.endswith(".rs"):
                some.update(users)
                reasons.append(f"{f}: test support -> the tests that use it: {', '.join(users)}")
            else:
                own = sorted(tid for tid, t in targets.items() if t["pkg"] == pkg and t["kind"] == "test")
                some.update(own)
                reasons.append(f"{f}: test support/fixture -> every test binary of {pkg}")
                other = [u for u in users if targets[u]["pkg"] != pkg]
                if other:
                    some.update(other)
                    reasons.append(f"{f}: included with #[path] by {', '.join(other)}")
        elif rel.startswith("benches/"):
            reasons.append(f"{f}: a benchmark -> nothing (scripts/bench-check.sh)")
        else:
            deps = dependents(pkg)
            whole_pkgs |= deps
            reasons.append(f"{f}: {pkg} -> {pkg} and its dependents: {', '.join(sorted(deps - {pkg})) or 'none'}")
        continue
    top = f.split("/")[0]
    word = None
    if top in ("examples", "editors", "changes", "scripts"):
        word = top
    elif f == "CHANGELOG.md":
        word = f
    elif f.startswith("docs/book/"):
        word = "book"
    elif top == "docs":
        word = "docs"
    elif top == "tests" and "/" in f[6:]:
        word = f.split("/")[1]
    if word:
        hit = consumers(word)
        some |= hit
        reasons.append(f"{f}: tests naming \"{word}\" -> {', '.join(sorted(hit)) or 'none'}")
    else:
        reasons.append(f"{f}: not read by any test -> nothing")

# One line per outcome: "first file (+N more): outcome".
grouped = {}
for r in reasons:
    f, _, outcome = r.partition(": ")
    grouped.setdefault(outcome, []).append(f)
reasons = [
    (fs[0] + (f" (+{len(fs) - 1} more)" if len(fs) > 1 else "") + ": " + o)
    for o, fs in grouped.items()
]
plan = {"full": full, "reasons": reasons}
if not full:
    # A binary already covered by a whole package needs no filter of its own.
    some = {s for s in some if s in targets and targets[s]["pkg"] not in whole_pkgs}
    packages = sorted(whole_pkgs | {targets[s]["pkg"] for s in some})
    parts = [f"package(={p})" for p in sorted(whole_pkgs)]
    by_binary = defaultdict(list)    # binary id -> modules of `it` (None: all of it)
    for s in sorted(some):
        by_binary[targets[s]["binary"]].append(targets[s]["module"])
    for b, mods in sorted(by_binary.items()):
        every = {t["module"] for t in targets.values() if t["binary"] == b}
        if None in mods or set(mods) == every:
            parts.append(f"binary_id(={b})")
        else:
            parts.append(f"(binary_id(={b}) & test(/^({'|'.join(mods)})::/))")
    selected_in = sorted(
        b for b in IN_PROCESS
        if b.split("::")[0] in whole_pkgs or b in some
    )
    doc = sorted(p for p in whole_pkgs if any(t["kind"][0] == "lib" for t in pkgs[p]["targets"]))
    plan.update(packages=packages, filter=" | ".join(parts), in_process=selected_in, doc=doc)
print(json.dumps(plan))
PY
[ -s "$METADATA.plan" ] || { echo "scripts/affected-tests.sh: choosing tests failed" >&2; exit 2; }

PLAN="$(cat "$METADATA.plan")"
field() { python3 -c 'import json,sys; v=json.loads(sys.argv[1]).get(sys.argv[2]); print(" ".join(v) if isinstance(v, list) else ("" if v is None else v))' "$PLAN" "$1"; }

echo "== affected tests (changes since $(git rev-parse --short "$BASE" 2>/dev/null || echo "$BASE"))"
python3 -c 'import json,sys; [print("   " + r) for r in json.loads(sys.argv[1])["reasons"]]' "$PLAN"

if [ "$(field full)" = True ]; then
    echo "== chose: everything"
    [ $DRY = 1 ] && exit 0
    exec "$ROOT/scripts/test.sh"
fi

PACKAGES="$(field packages)"
FILTER="$(field filter)"
IN_PROCESS="$(field in_process)"
DOC="$(field doc)"

if [ -z "$PACKAGES" ]; then
    echo "== chose: nothing (no test reads the changed files)"
    exit 0
fi
echo "== chose"
echo "   packages:   $PACKAGES"
echo "   nextest:    $FILTER"
echo "   in-process: ${IN_PROCESS:-none}"
echo "   doc tests:  ${DOC:-none}"
[ $DRY = 1 ] && exit 0

ARGS=()
for p in $PACKAGES; do ARGS+=(-p "$p"); done
exec "$ROOT/scripts/test.sh" "${ARGS[@]}" -E "$FILTER" --in-process "$IN_PROCESS" --doc "$DOC"
