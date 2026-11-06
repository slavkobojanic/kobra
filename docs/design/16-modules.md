# Modules and Visibility

## Decision

Modules and the visibility of declarations follow **Java's package model**. **Members follow their type:** a member is visible wherever the declaration that holds it is visible, unless it is marked `private`.

```k
// src/shop/cart/CartService.k        → module shop.cart
import shop.catalog.Product
import shop.catalog.ProductService

public service CartService {             // visible to every module
    @Inject let products: ProductService  // always private: dependencies are never API

    @State var items: List<CartItem> = []  // public, because CartService is
    private var retryCount = 0             // visible only inside CartService

    func add(product: Product) { ... }     // public, because CartService is
}

struct CartLine { ... }                   // no modifier: visible within shop.cart only
```

## Modules

- **A directory is a module.** The module name comes from the path, so files never declare `package`.
- **Circular imports between modules are a compile error.** Test files don't take part in cycle detection, because nothing imports them (see [Test-only modules](#test-only-modules)).

## Visibility of declarations

| Modifier | Visible to |
|---|---|
| `public` | every module |
| *(none)* | the same module (directory), like Java's package-private |
| `private` | only the file that declares it |

- There is no `protected`, because there is no inheritance (see [08-types.md](08-types.md)).
- **A file has at most one `public` declaration, and its name matches the file name:** `CartService.k` holds `public service CartService`. Any type can be found from its name alone. Extensions don't count, because they have no name (see [Extensions](#extensions)).
- Other declarations in the file are helpers, visible within the module.

## Visibility of members

- **A member is visible wherever its enclosing declaration is visible.** Fields, functions, computed properties, static members and `init` of a `public` type are public; those of a package-private type are visible within its module.
- **`private` is the only member modifier.** It restricts a member to the body of its enclosing declaration.
- **`public` on a member is redundant.** It is a compile warning with a machine-applicable fix, and `k fmt` removes it, so there is one spelling.
- **`@Inject` fields are always private to their declaration.** Writing `public` on one is a compile error; `private` is redundant and `k fmt` removes it. A type's dependencies are never part of its API, so importers can't read `verifier.config`, and fakes never have to declare their target's dependencies ([21-testing.md](21-testing.md)).
- **Writes are restricted separately from visibility.** A service's `var` fields, `@State` or not, are writable only by the service's own methods ([10-services.md](10-services.md)). A struct's `var` fields are writable by whoever holds a `var` copy, because structs have value semantics ([08-types.md](08-types.md)).
- Enum cases and the `rpc`s of a protocol can't be `private`.

### Why

- A declaration's visibility is stated once, on the type. Members are API unless hidden, which is the common case, and `private` marks the exception that a reviewer needs to see.
- One spelling per meaning ([01-principles.md](01-principles.md)): with a default of public members, a `public` keyword on a member could only ever restate it.

## Extensions

An extension has no name, so it can't be imported. Its members follow the extension's own modifier:

| Extension | Its members are visible |
|---|---|
| `public extend Place { … }` | wherever `Place` is visible, with no import |
| `extend Place { … }` | within the extension's module |

- `private extend` is a compile error.
- Two visible extensions that add a member with the same name to the same type are a compile error at the second.
- A file that holds only extensions is named after their purpose (`Samples.k`).
- The rule is the same in test-only modules. Because only test files can see a test-only module, the public extensions in one are visible to every test file and to no production code.

## Test-only modules

- **A directory named `testing`, at any depth, is a test-only module.** Only `*.test.k` files and other test-only modules may import from it. It is never part of `k build` output, and it may import `k.test`.
- Shared fakes and fixtures go there (`src/testing/FakeWeatherService.k`). Helpers used by a single test file stay in that file.
- **Declarations in a `*.test.k` file** (helper functions, helper components, wire-format builders) are visible only to the test files of the same module.

## Imports

- **Each import names one declaration:** `import shop.catalog.Product`. There are no wildcards, so the source of every name is visible at the top of the file.
- The language server adds and removes imports automatically.
- **Built-in elements and core types** (`String`, `List`, `Map`, `Color`, `Bytes`, `Url`, `Secret`, units; see [20-standard-library.md](20-standard-library.md)) need no import. Neither do the enums the compiler generates for the app (`Page`, `Endpoint`, `ThemeVariant`).
