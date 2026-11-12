# Upgrading K

How to upgrade.

<a id="draggable-kind-first"></a>
### `Draggable` takes its kind first

Reorder the arguments.

## Every minor's migrations

<!-- gen:upgrading -->
## Upgrading to 0.4

<!-- release 0.4.0 -->
- **Breaking.** `Draggable` takes its kind first: `Draggable(kind:payload:)`.

  A second paragraph explains the edge case. Migration: by hand, as [its section](#draggable-kind-first) describes. See [Charts](../apps/charts.md), [Functions](../language/functions.md).
- **Deprecated.** `sum(of:)` is deprecated; call `total(of:)`. Migration: `k upgrade` applies the fix of K1401. See [Functions](../language/functions.md).

## Upgrading to 0.3

<!-- release 0.3.0 -->
Nothing to migrate: this release has no breaking changes or deprecations.

<!-- /gen -->
