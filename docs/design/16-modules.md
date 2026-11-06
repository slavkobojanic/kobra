# Modules and Visibility

## Decision

Modules and visibility follow **Java's package model**.

```k
// src/shop/cart/CartService.k        → module shop.cart
import shop.catalog.Product
import shop.catalog.ProductService

public service CartService {             // visible to every module
    @Inject let products: ProductService

    private var retryCount = 0      // visible only inside CartService

    func add(product: Product) { ... }
}

struct CartLine { ... }                   // no modifier: visible within shop.cart only
```

## Modules

- **A directory is a module.** The module name comes from the path, so files never declare `package`.
- **Circular imports between modules are a compile error.**

## Visibility

| Modifier | Visible to |
|---|---|
| `public` | every module |
| *(none)* | the same module (directory), like Java's package-private |
| `private` | only the enclosing declaration |

- There is no `protected`, because there is no inheritance (see [08-types.md](08-types.md)).
- **A file has at most one `public` declaration, and its name matches the file name:** `CartService.k` holds `public service CartService`. Any type can be found from its name alone.
- Other declarations in the file are helpers, visible within the module.

## Imports

- **Each import names one declaration:** `import shop.catalog.Product`. There are no wildcards, so the source of every name is visible at the top of the file.
- The language server adds and removes imports automatically.
- **Built-in elements and core types** (`String`, `List`, `Map`, `Color`, units) need no import.
