# Packages

Third-party K code is shared as **git-based packages**, like Go modules. A package's name is its module
path, and its versions are git tags.

```bash
k add github.com/acme/charts@v1.2.0
```

```k
import github.com/acme/charts.LineChart
```

## How packages work

- **Versions are git tags.** `k add` records the dependency in `k.toml`, and `k.lock` records the exact
  commit and a content hash for every package. Builds are reproducible, and a re-tagged release is
  detected.
- **Packages are distributed as K source**, not as compiled JavaScript. The compiler type-checks and
  tree-shakes them, and enforces layering and the server/client boundary on them exactly as on your own
  code. A package can't smuggle a database call into your client bundle.
- **A package's `public` declarations are its API.** Module-private declarations stay hidden.
- `k remove` takes a dependency out again.

## Short imports with aliases

Module paths can be long. An alias in `k.toml` allows short imports:

```toml
[dependencies]
"github.com/acme/charts" = "v1.2.0"

[aliases]
charts = "github.com/acme/charts"
```

```k
import charts.LineChart
```

## Writing a package

A project without an `App.k` is a library package. Create one with:

```bash
k new --library github.com/acme/charts
```

> **Proposal.** `k new --library` follows `docs/design/proposals/project-creation.md`.

Publishing is pushing a git tag such as `v1.2.0`. Document every public declaration with `///` comments:
in a package, `k check` warns about a public declaration without one, because it is part of your API.

Packages that integrate a vendor, such as a payment provider or an email API, implement the standard
library's provider interfaces, such as `EmailProvider`. An app then chooses the implementation with a `use`
rule in `App.k`, and everything else keeps working against the interface.

## A registry

> **Planned.** A hosted package registry is deferred. When it arrives, it will be another way of resolving
> module paths, so import syntax won't change.

## No npm

> **Planned.** Typed npm interop, with bindings generated from `.d.ts` files, is deferred and may be added
> later. Any design for it would keep JavaScript calls behind services, as the layering rules already do
> for protocols.

**K has no npm support and no JavaScript interop in user code.** All application code is K, so the
compiler's guarantees (types, layering, the server/client boundary, reactivity) hold everywhere, with no
untyped holes. The standard library wraps the web platform instead: HTTP, storage, WebSockets, canvas,
clipboard, media, crypto, workers and the rest. Only the standard library can call JavaScript, through an
internal mechanism that user code can't access.
