# Proposal: `k.toml` Project Manifest

> **Status: proposal.** Not yet accepted into the design. First used by `examples/weather`.

## Problem

[18-packages.md](../18-packages.md) mentions `k.toml` (for package aliases) and [13-tooling.md](../13-tooling.md) mentions `k.lock`, but neither defines the manifest's format or where a project's files live.

## Proposal

Every K project, app or package, has a `k.toml` at its root. It is the only configuration file, and it holds only what can't be expressed in K code.

```toml
[package]
name = "weather"                 # module path prefix for a package; display name for an app
version = "0.1.0"                # semantic version; for packages, matches the git tag without "v"
description = "A weather dashboard backed by the Open-Meteo APIs."
k = "0.1"                        # minimum K toolchain version

[dependencies]
"github.com/acme/charts" = "v1.2.0"     # module path = git tag (written by `k add`)

[aliases]
charts = "github.com/acme/charts"       # enables `import charts.LineChart`
```

### Sections

| Section | Contents |
|---|---|
| `[package]` | `name`, `version`, `description`, `k` (minimum toolchain version). All but `description` are required. |
| `[dependencies]` | Module path → git tag. `k add` and `k remove` edit it; `k.lock` pins commits and hashes. |
| `[aliases]` | Short name → module path, as described in [18-packages.md](../18-packages.md). |

### Fixed project layout (not configurable)

```
k.toml        manifest
k.lock        package lock (generated)
App.k         app declaration (apps only; see app-structure.md)
src/          source; each directory is a module (src/weather/Place.k → weather.Place)
*.lock        protocol.lock, routes.lock, schema.lock (generated)
dist/         build output (generated, not committed)
```

- **The layout is fixed**, like the formatter: there is no `source = "src"` key. Every K repository looks the same.
- **A project with an `App.k` is an app; one without is a library package.**
- **No build scripts, no environment variables, no secrets** in `k.toml`. Per-environment choices are `use … in .production` rules in `App.k` (see [10-services.md](../10-services.md)). Secrets need their own design, deliberately kept out of a committed file.
- **No tool settings.** The formatter, compiler and test runner have no options to configure.

### Extension: migrations and local environment files

*First used by `examples/shop`.*

The fixed layout also contains:

```
migrations/   database migrations, NNNN_description.k (see migrations.md); not a module
.env          optional, git-ignored; loaded by `k dev` only (see config-and-secrets.md)
```

This keeps the rule above intact: configuration values never go in `k.toml`. A project commits `.env.example` to document what `.env` should contain.

## Why

- TOML is readable, has comments, and is familiar from Cargo, which the toolchain is built with.
- Keeping the manifest minimal follows "one obvious way": anything that *can* be K code (DI choices, layouts, metadata) *is* K code, where the compiler checks it.
