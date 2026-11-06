# Packages

## Decision

Third-party K code is shared as **git-based packages, like Go modules.** There is no registry for now.

```bash
k add github.com/acme/charts@v1.2.0
```

```k
import github.com/acme/charts.LineChart
```

## Rules

- **Versions are git tags.** `k.lock` records the exact commit and a content hash for every package, so builds are reproducible and a re-tagged release is detected.
- **Packages are distributed as K source,** not compiled JavaScript. The compiler type-checks and tree-shakes them, and enforces layering and the server/client boundary on them like application code.
- **A package's `public` declarations are its API.** Package-private declarations stay hidden (see [16-modules.md](16-modules.md)).
- **Package names are module paths.** An alias in `k.toml` allows short imports such as `import charts.LineChart`.

## Later

A hosted registry can be added later without changing import syntax. It would be another way of resolving module paths.
