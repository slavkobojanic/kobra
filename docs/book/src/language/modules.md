# Modules and visibility

K organizes code the way Java does: **a directory is a module**, and visibility is decided per module.
There is nothing to configure, and no file declares which module it belongs to.

## Modules come from directories

```text
src/
  shop/
    cart/
      CartService.k        module shop.cart
      CartLine.k           module shop.cart
    catalog/
      Product.k            module shop.catalog
      ProductService.k     module shop.catalog
```

The module name is the directory path under `src/`, with dots: `src/shop/cart/` is `shop.cart`.

## One public declaration per file

**A file has at most one `public` declaration, and the file is named after it.** `CartService.k` holds
`public service CartService`. Any public type can therefore be found from its name alone.

Other declarations in the same file are helpers. They are visible within the module, but not outside it:

```k
// src/shop/cart/CartService.k        → module shop.cart
import shop.catalog.Product
import shop.catalog.ProductService

public service CartService {             // visible to every module
    @Inject let products: ProductService

    private var retryCount = 0           // visible only inside CartService

    func add(product: Product) { … }
}

struct CartLine { … }                    // no modifier: visible within shop.cart only
```

## Visibility

| Modifier | Visible to |
|---|---|
| `public` | every module |
| *(none)* | the same module, like Java's package-private |
| `private` | only the enclosing declaration |

The default, module visibility, is the right one for most declarations: everything a module needs
internally is shared within it, and nothing leaks out. Make a declaration `public` when other modules
need it, and `private` when even the rest of its module shouldn't touch it.

The same modifiers apply to members. A public struct's fields are visible outside its module only if they
are `public` too.

There is no `protected`, because there is no inheritance.

## Imports

**Each import names exactly one declaration:**

```k
import shop.catalog.Product
import shop.catalog.ProductService
import k.ui.Theme
import k.collections.LruCache
```

- There are no wildcard imports, so the origin of every name is visible at the top of the file.
- Imports come before any declaration in the file.
- Declarations in the same module need no import.
- **Built-in elements and core types** (`String`, `List`, `Map`, `Color`, units and the rest) need no
  import either. Neither do the generated `Page` enum and theme tokens.
- Packages are imported by their module path: `import github.com/acme/charts.LineChart`, or
  `import charts.LineChart` with an alias in `k.toml`. See [Packages](../apps/packages.md).

> **Planned.** The language server, which adds and removes imports automatically as you write code, is
> deferred.

## No cycles

**Circular imports between modules are a compile error.** If `shop.cart` imports from `shop.catalog`,
then `shop.catalog` can't import from `shop.cart`. Cycles are a sign that two modules are really one, or
that a shared piece belongs in a third module that both import.

Test files are the exception: since nothing imports a test file, tests don't take part in cycle
detection.

## Test-only modules

> **Proposal.** Test-only modules follow `docs/design/proposals/testing-support.md`.

A directory named `testing`, at any depth, is a **test-only module**. It holds shared fakes and fixtures.
Only test files and other test-only modules may import from it, and it never becomes part of a build.

```text
src/
  testing/
    FakeCartApi.k
    Samples.k
```

## Server-only code

Module visibility answers "who may use this declaration?". A second, independent rule answers "where may
it run?": **server-only declarations**, such as repositories, configurations and services that implement
protocols, can never be imported into code that runs in the browser. The compiler checks this boundary
across modules. See [Services and dependency injection](../apps/services.md#the-serverclient-boundary).
