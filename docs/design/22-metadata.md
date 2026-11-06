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
- **`canonical` takes a typed page,** so canonical URLs come from `routes.lock` ([12-routing.md](12-routing.md)) and can't drift. It renders the absolute URL, built from the public origin ([38-deployment.md](38-deployment.md)).
- **`status(_:)` sets the HTTP status of a server-rendered page.** It is allowed only in a `metadata` block inside `failed`, for example `status(.notFound)` for a missing product ([14-async-data.md](14-async-data.md#server-rendering-and-hydration)).
- **Metadata belongs to view pages.** Endpoint pages have no `metadata`, and app and layout defaults don't apply to them ([27-endpoint-pages.md](27-endpoint-pages.md)).
- **Every response whose rendering read `Session` carries `Cache-Control: private, no-store`,** so a shared cache never serves one user's page to another ([26-auth-and-sessions.md](26-auth-and-sessions.md#cookies-and-caching)).
- **Compile-time warnings:** a page with no `title`, a description over 160 characters.
- **App-wide entries go in `App.k`:** web manifest, theme color, default locale ([23-app-structure.md](23-app-structure.md)).
- **The `locale` in effect is the only locale.** Number, date and money formatting use the app's or page's `metadata` locale on the server, in the browser and in background jobs alike, never the browser's `navigator.language`, so server-rendered text and hydrated text agree ([34-money.md](34-money.md), [35-time.md](35-time.md)).
