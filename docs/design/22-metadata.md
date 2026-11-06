# Document Metadata

## Decision

Pages and layouts declare `<head>` content in a typed **`metadata { }` block**, next to `view`, `style` and `loading`.

```k
page Product {
    @Param let id: ProductId
    @Load let product: Product = try await products.find(id)

    metadata {
        title("{product.name} · Acme Shop")
        description(product.summary)
        canonical(.product(id: id))
        socialPreview(image: product.imageUrl, title: product.name)
        structuredData(.product(name: product.name, price: product.price))
    }

    view { ... }
}

layout Shop {
    metadata {
        title("Acme Shop")              // default for every page in this layout
        icon(.appIcon)
    }
}
```

## Rules

- **Entries are typed.** There are no raw `<meta name=...>` strings.
- **Metadata is reactive and rendered on the server,** so `@Load` values appear in the initial HTML for crawlers and link previews.
- **Layouts provide defaults; pages override them.** For each entry, the innermost declaration wins.
- **Stylesheets, scripts and fonts never appear in metadata.** The compiler emits CSS from `style` blocks, there is no third-party JavaScript, and theme `font` tokens produce preload tags automatically.
- **`canonical` takes a typed page,** so canonical URLs come from `routes.lock` ([12-routing.md](12-routing.md)) and can't drift.
- **Compile-time warnings:** a page with no `title`, a description over 160 characters.
- **App-wide entries go in `App.k`:** web manifest, theme color, default locale.
