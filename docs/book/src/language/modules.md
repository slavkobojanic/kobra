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

<!-- The declarations the samples in this chapter use; checked, not rendered.

```k
// src/shop/cart/CartItem.k
import shop.catalog.Product

/// One line of the cart.
public struct CartItem {
    /// The product in this line.
    let product: Product
}
```

```k
// src/shop/catalog/Product.k
/// A product in the catalog.
public struct Product {
    /// The product's name.
    let name: String
    /// The unit price.
    let price: Money
}
```

```k
// src/shop/catalog/ProductService.k
/// Looks products up.
public service ProductService {
    /// The products on sale.
    func all() -> List<Product> {
        return []
    }
}
```
-->

## One public declaration per file

**A file has at most one `public` declaration, and the file is named after it.** `CartService.k` holds
`public service CartService`. Any public type can therefore be found from its name alone. Extensions
don't count, because they have no name.

Other declarations in the same file are helpers. They are visible within the module, but not outside it:

```k
// src/shop/cart/CartService.k        → module shop.cart
import shop.catalog.Product
import shop.catalog.ProductService

public service CartService {               // visible to every module
    @Inject let products: ProductService    // always private: dependencies are never API

    @State var items: List<CartItem> = []   // public, because CartService is
    private var retryCount = 0              // visible only inside CartService

    func add(product: Product) {            // public, because CartService is
        items.append(CartItem(product: product))
    }
}

struct CartLine {                          // no modifier: visible within shop.cart only
    let quantity: Int
}
```

## Visibility of declarations

| Modifier | Visible to |
|---|---|
| `public` | every module |
| *(none)* | the same module (directory), like Java's package-private |
| `private` | only the file that declares it |

The default, module visibility, is the right one for most declarations: everything a module needs
internally is shared within it, and nothing leaks out. Make a declaration `public` when other modules
need it, and `private` when even the rest of its module shouldn't touch it.

There is no `protected`, because there is no inheritance.

## Visibility of members

**A member is visible wherever the declaration that holds it is visible, unless it is marked
`private`.** The visibility of a type is stated once, on the type:

- Fields, functions, computed properties, static members and `init` of a `public` type are public. Those
  of a module-private type are visible within its module.
- **`private` is the only member modifier.** It restricts a member to the body of its enclosing
  declaration.
- **`public` on a member is redundant.** It is a warning (`K1302`) with a machine-applicable fix,
  which `k fmt` will apply (**Planned**), so there is one spelling.
- **`@Inject` fields are always private** to the declaration that injects them. Writing `public` on one
  is a compile error, and `private` is redundant. A type's dependencies are never part of its API, so
  other code can't read `verifier.config`, and a fake never has to declare its target's dependencies.
- **Writes are restricted separately from visibility.** A service's `var` fields, `@State` or not, are
  writable only by the service's own methods. A struct's `var` fields are writable by whoever holds a
  `var` copy, because structs are values.
- Enum cases and the `rpc`s of a protocol can't be `private`.

Members are API unless hidden, which is the common case, and `private` marks the exception a reviewer
needs to see.

## Extensions

An extension has no name, so it can't be imported. Its members follow the extension's own modifier:

| Extension | Its members are visible |
|---|---|
| `public extend Place { … }` | wherever `Place` is visible, with no import |
| `extend Place { … }` | within the extension's module |

- `private extend` is a compile error.
- Two visible extensions that add a member with the same name to the same type are a compile error at
  the second.
- A file that holds only extensions is named after their purpose, for example `Samples.k`.

## Imports

**Each import names exactly one declaration:**

```k
import shop.catalog.Product
import shop.catalog.ProductService
import k.ui.Theme
import k.time.Clock
```

- There are no wildcard imports, so the origin of every name is visible at the top of the file.
- Imports come before any declaration in the file.
- Declarations in the same module need no import.
- **Built-in elements and core types** (`String`, `List`, `Map`, `Color`, `Bytes`, `Url`, `Secret`,
  units and the rest) need no import either. Neither do the enums the compiler generates for the app
  (`Page`, `Endpoint`, `ThemeVariant`) or theme tokens.
- Git packages are imported by their URL-prefixed module path: `import github.com/acme/charts.LineChart`,
  or `import charts.LineChart` with an alias in `k.toml`. A library of your own workspace keeps its
  modules' own paths (`import pay.Payments`). See [Packages](../apps/packages.md).

The language server manages imports as you write code: choosing a completion from another module adds
its import, a missing import is a quick fix, and an unused import is greyed out with a quick fix that
removes it. See [Editor setup](../getting-started/editor-setup.md).

## No cycles

**Circular imports between modules are a compile error.** If `shop.cart` imports from `shop.catalog`,
then `shop.catalog` can't import from `shop.cart`. Cycles are a sign that two modules are really one, or
that a shared piece belongs in a third module that both import.

Test files are the exception: since nothing imports a test file, tests don't take part in cycle
detection.

## Test-only modules

A directory named `testing`, at any depth, is a **test-only module**. It holds shared fakes and fixtures.
Only test files and other test-only modules may import from it, it may import `k.test`, and it never
becomes part of a build.

```text
src/
  testing/
    FakeWeatherService.k
    Samples.k
```

- **Fixtures are static members added by a `public extend`** in a test-only module. By the extension
  rule above they are visible to every test file with no import, and to no production code.
- **Declarations in a `*.test.k` file**, such as helper functions and helper components, are visible
  only to the test files of the same module.

## Server-only code

Module visibility answers "who may use this declaration?". A second, independent rule answers "where may
it run?": **server-only declarations**, such as entities, repositories, configs, endpoint pages and services
that implement protocols, can never be imported into code that runs in the browser. The compiler checks this boundary
across modules. See [Services and dependency injection](../apps/services.md#the-serverclient-boundary).
