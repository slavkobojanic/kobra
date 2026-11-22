# Packages

## Decision

Third-party Kobra code is shared as **git-based packages, like Go modules.** There is no registry for now.

```bash
k add github.com/acme/charts@v1.2.0
```

```k
import github.com/acme/charts.LineChart
```

## Rules

- **Versions are git tags.** `k.lock` records the exact commit and a content hash for every package, so builds are reproducible and a re-tagged release is detected.
- **Packages are distributed as Kobra source,** not compiled JavaScript. The compiler type-checks and tree-shakes them, and enforces layering and the server/client boundary on them like application code.
- **A package's `public` declarations, and their non-private members, are its API.** Package-private declarations stay hidden (see [16-modules.md](16-modules.md)). `k check` warns on API without a doc comment ([39-doc-comments.md](39-doc-comments.md)).
- **A git package's name is the prefix of its module paths.** The name is the package's `[package] name` in `k.toml` ([24-projects.md](24-projects.md)), a URL such as `github.com/acme/charts`, so its modules are `github.com/acme/charts.…` everywhere: the URL keeps packages from different authors from colliding. Because those paths are long, **an alias in `[aliases]` gives a git package a short prefix**: with `charts = "github.com/acme/charts"`, `import charts.LineChart` imports `github.com/acme/charts.LineChart`.
- **A workspace library keeps its own module paths.** A library in the same workspace ([40-workspaces-and-services.md](40-workspaces-and-services.md#decided-while-implementing)) is the team's own code, so its modules are imported exactly as they are written in it (`pay-contracts/src/pay/Payments.k` is `pay.Payments`), with no prefix and no alias. Nothing outside the workspace can collide with them, and inside it **module paths are unique**: two packages of one build declaring the same module is `K8106`, so an import always names exactly one module.
- **The git tag is the only version.** A package's `k.toml` has no `version` key, so the version can't disagree with the tag.

### Decided while implementing workspaces

Workspace libraries were built first, and their modules kept their own paths, so shop imports paybox's contracts as `import pay.Payments`. A prefix would have made every import of a team's own contracts carry the library's name, to prevent a collision that `K8106` already rules out within one build. Git packages differ because they come from anywhere: their URL prefix is what keeps two authors' `charts` modules apart, and the alias is what keeps their imports short. Git packages and `k add` are not built yet; a git tag in `[dependencies]` is reported as not resolvable (`K8102`).

## Later

A hosted registry can be added later without changing import syntax. It would be another way of resolving module paths.
