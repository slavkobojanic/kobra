# Document metadata

Pages and layouts declare their `<head>` content in a typed **`metadata { }` block**, next to `view`,
`style` and `loading`.

```k
page Product {
    @Param let id: ProductId
    @Inject let products: ProductService

    @Load let product: Product = try await products.find(id)

    metadata {
        title("{product.name} · Acme Shop")
        description(product.summary)
        canonical(.product(id: id))
        socialPreview(image: product.imageUrl, title: product.name)
        structuredData(.product(name: product.name, price: product.price))
    }

    view { … }

    loading { ProductSkeleton() }
}

layout Shop {
    @Slot let content: View

    metadata {
        title("Acme Shop")              // the default for every page in this layout
        icon(.appIcon)
    }

    view { … }
}
```

## Rules

- **Entries are typed.** There are no raw `<meta name=…>` strings.
- **Metadata is reactive and rendered on the server**, so `@Load` values appear in the initial HTML for
  search engines and link previews.
- **Layouts provide defaults; pages override them.** For each entry, the innermost declaration wins.
- **`canonical` takes a typed page**, so canonical URLs come from `routes.lock` and can't drift.
- **Stylesheets, scripts and fonts never appear in metadata.** The compiler emits CSS from `style` blocks,
  there is no third-party JavaScript, and theme `font` tokens produce preload tags automatically.

The compiler warns about a page with no `title`, and about a description longer than 160 characters.

## App-wide entries

App-wide entries, such as the web manifest, the theme color and the default locale, go in `App.k`:

```k
public app Weather {
    metadata {
        manifest(name: "Weather", shortName: "Weather")
        themeColor(.primary)
        locale(.english)
    }
}
```

> **Proposal.** The `app` declaration follows `docs/design/proposals/app-structure.md`. See
> [The app declaration](app.md).
