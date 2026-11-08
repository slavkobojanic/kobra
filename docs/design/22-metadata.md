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
- **`status(_:)` sets the HTTP status of a server-rendered page.** It is allowed only in a `metadata` block inside `failed`, for example `status(.notFound)` for a missing product ([14-async-data.md](14-async-data.md#server-rendering-and-hydration)). When `failed` renders, the page-level block isn't evaluated ([below](#metadata-when-a-load-fails)).
- **Metadata belongs to view pages.** Endpoint pages have no `metadata`, and app and layout defaults don't apply to them ([27-endpoint-pages.md](27-endpoint-pages.md)).
- **Every response whose rendering read `Session` carries `Cache-Control: private, no-store`,** so a shared cache never serves one user's page to another ([26-auth-and-sessions.md](26-auth-and-sessions.md#cookies-and-caching)).
- **Compile-time warnings:** a page with no `title`, a description over 160 characters. A page has a title when its own `metadata`, the layout it renders in or the app's sets one, so a default in `App.k` or a layout covers every page under it; a `title` inside `failed` doesn't count. Only a program with an `app` is checked, since only an app renders documents. The description is measured in characters, and only when it is a literal.
- **App-wide entries go in `App.k`:** web manifest, theme color, default locale ([23-app-structure.md](23-app-structure.md)).
- **The `locale` in effect is the only locale.** Number, date and money formatting use the app's or page's `metadata` locale on the server, in the browser and in background jobs alike, never the browser's `navigator.language`, so server-rendered text and hydrated text agree ([34-money.md](34-money.md), [35-time.md](35-time.md)).

## Metadata when a load fails

```k
page Product {
    @Param let id: ProductId
    @Load let product: Product = try await products.find(id)

    metadata {
        title("{product.name} · Acme Shop")       // not evaluated when failed renders
    }

    view { ... }

    loading { ProductSkeleton() }

    failed(let error) {
        match error {
            case .notFound {
                metadata {
                    title("Product not found · Acme Shop")
                    status(.notFound)
                }

                NotFound()
            }

            default {
                RetryPanel()
                    .on(retry: reload)
            }
        }
    }
}
```

- **When `failed` renders, the page-level `metadata` block is not evaluated.** It usually reads `@Load` values, which don't exist when a load failed.
- **What applies instead:** the `metadata` block inside `failed`, if there is one, and the defaults from `App.k` and the enclosing layouts, with the usual innermost-wins rule. So the `default` arm above, which declares no `metadata`, shows the layout's title.
