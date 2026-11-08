# Packages

Third-party K code is shared as **git-based packages**, like Go modules. A package's name is a URL that
prefixes its module paths, and its versions are git tags.

```bash
k add github.com/acme/charts@v1.2.0
```

```k,ignore
import github.com/acme/charts.LineChart
```

## How packages work

- **Versions are git tags.** `k add` records the dependency in `k.toml`, and `k.lock` records the exact
  commit and a content hash for every package. Builds are reproducible, and a re-tagged release is
  detected.
- **Packages are distributed as K source**, not as compiled JavaScript. The compiler type-checks and
  tree-shakes them, and enforces layering and the server/client boundary on them exactly as on your own
  code. A package can't smuggle a database call into your client bundle.
- **A package's `public` declarations, and their non-private members, are its API.** Module-private
  declarations and `private` members stay hidden.
- **A git package's name prefixes its module paths.** The `name` in its `k.toml` is a URL, so the
  modules of two authors' `charts` packages never collide: `github.com/acme/charts.LineChart`.
- **The git tag is the only version.** A package's `k.toml` has no `version` key, so the version can't
  disagree with the tag.
- `k remove` takes a dependency out again.

> **Planned.** Packages and `k add` aren't in the first compiler releases.

## Short imports with aliases

URL prefixes are long. An alias in `k.toml` gives a git package a short prefix:

```toml
[dependencies]
"github.com/acme/charts" = "v1.2.0"

[aliases]
charts = "github.com/acme/charts"
```

```k,ignore
import charts.LineChart
```

## Writing a package

A project without an `App.k` is a library package. Create one with:

```bash
k new --library github.com/acme/charts
```

Publishing is pushing a git tag such as `v1.2.0`. Document every public declaration with `///` comments:
in a package, `k check` warns about API without one (see [doc comments](../language/basics.md#comments)).

Packages that integrate a vendor, such as a payment provider or an email API, implement the standard
library's provider interfaces, such as `EmailProvider`. An app then chooses the implementation with a `use`
rule in `App.k`, and everything else keeps working against the interface.

## A registry

A hosted package registry is deferred by the design. If one is added, it will be another way of
resolving module paths, so import syntax won't change.

## No npm

**K has no npm support and no JavaScript interop in user code.** All application code is K, so the
compiler's guarantees (types, layering, the server/client boundary, reactivity) hold everywhere, with no
untyped holes. The standard library wraps the web platform instead: HTTP, storage, WebSockets, canvas,
clipboard, media, crypto, workers and the rest. Only the standard library can call JavaScript, through an
internal mechanism that user code can't access.

Typed npm interop, with bindings generated from `.d.ts` files, is deferred by the design and may be added
later. Any design for it would keep JavaScript calls behind services, as the layering rules already do
for protocols.

## Libraries in a workspace

Libraries you don't publish, such as the contracts between your own services, live in the same
[workspace](workspaces.md) as the apps that use them and are depended on with `name = { workspace = true
}`. That works today, without `k add`. **Their modules keep their own paths,** with no prefix and no
alias: `pay-contracts/src/pay/Payments.k` is imported as `import pay.Payments`. Two packages of one build
can't declare the same module (`K8106`), so every module path still names exactly one module.
