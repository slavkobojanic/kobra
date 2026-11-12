# Changelog

Every released version, newest first.

## 0.4.0-rc.1 (pre-release of 0.4.0, 2026-11-01)

### Highlights

- Charts draw grouped bars.

### Breaking changes

- `Draggable` takes its kind first: `Draggable(kind:payload:)`.

  A second paragraph explains the edge case. Migration: by hand, as [the upgrade guide](docs/book/src/apps/upgrading.md#draggable-kind-first) describes. See [Charts](docs/book/src/apps/charts.md), [Functions](docs/book/src/language/functions.md).

### Deprecations

- `sum(of:)` is deprecated; call `total(of:)`. Migration: `k upgrade` applies the fix of K1401. See [Functions](docs/book/src/language/functions.md).

### Standard library

- `BarChart` draws grouped bars: `BarChart(series:)` takes several series. See [Charts](docs/book/src/apps/charts.md).

### Fixes

- An empty chart no longer crashes the page.

## 0.3.0 (2026-09-20)

No user-visible changes.
