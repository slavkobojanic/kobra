# Pages and Routing

## Decision

Routes are declared in code, not derived from the file system. **Developers don't write URLs.** Pages are typed values, and the compiler generates the URLs.

```k
page Product requires Authenticated {
    @Param let id: ProductId                    // required
    @Param let tab: ProductTab = .overview      // optional

    @Inject let products: ProductService

    view { ... }
}

layout Shop {
    @Slot let content: View

    view {
        NavBar()

        main { content }

        Footer()
    }
}
```

- `page` and `layout` are declaration keywords.
- Pages and layouts follow the same injection rules as components: they inject services only.
- Layouts attach with `@Layout(Shop)` on a page, or to a group of pages in `App.k`.

## Navigation

Pages are cases of a **`Page` enum that the compiler generates**. This avoids name clashes (`page Product` vs `struct Product`). Navigation uses implicit member syntax and the named-or-chained rule:

```k
link(to: .product(id: product.id)) { product.name }

Router.navigate(to: .product(id: product.id).tab(.reviews))
```

- Navigating to a page that doesn't exist, or with missing or wrongly typed parameters, is a compile error.
- `link(url: "https://…")` is for external links.

## Parameters

- There is only `@Param`. The developer never chooses between path and query.
- Parameters are parsed into their types. An invalid value (for example, an `id` that isn't a valid `ProductId`) gives a 404.

## Guards: `requires`

```k
page Admin requires Authenticated, Role(.admin) { ... }
```

- Guards are the same middleware types used on protocols (see [11-protocols-and-rpc.md](11-protocols-and-rpc.md)), so `Authenticated` is one type everywhere.
- `requires` reads as prose. The word `validation` is reserved for form validation.

## URLs

### Default URLs

URLs are derived from the page name and its parameters:

| Declaration | URL |
|---|---|
| `page Pricing` | `/pricing` |
| `page Product` with required `id` | `/product/42` |
| …with optional `tab` set to `.reviews` | `/product/42?tab=reviews` |
| …with `tab` left at its default | `/product/42` |

- **Required parameters become path segments,** in declaration order.
- **Optional parameters go in the query string,** and are omitted when equal to their default. Putting optional parameters in the path is ambiguous when there is more than one (`/product/42//newest`).

### Named URLs

When something outside the app depends on a URL (SEO, webhooks, OAuth callbacks, `/.well-known/*`), the page names it with `at`:

```k
page StripeWebhook at "/webhooks/stripe" { ... }

page Pricing at "/pricing" { ... }

page ProductDetail at "/product/{id}/{tab}" { ... }     // optional parameters in the path, explicitly
```

Named URLs never change when the page is renamed.

### `routes.lock` and redirects

- The compiler records every page's URL in **`routes.lock`**.
- **Renaming an unnamed page changes its URL, and an automatic permanent redirect from the old URL is added.** URLs always match the current names, and old links keep working.
- Duplicate or conflicting URLs are compile errors.
- `k routes` prints the full route table, and the language server shows each page's URL inline.
