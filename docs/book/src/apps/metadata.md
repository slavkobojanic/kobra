# Document metadata

Pages and layouts declare their `<head>` content in a typed **`metadata { }` block**, next to `view`,
`style` and `loading`.

<!-- The declarations the samples in this chapter use; checked, not rendered.

```k
// src/store/Context.k
import k.crypto.Uuid

struct Product implements Codable {
    let id: Uuid
    let name: String
    let summary: String
    let imageUrl: Url
    let price: Money
}

enum CatalogError implements Error, Codable {
    case notFound
    case unavailable
}

service ProductService {
    func find(_ id: Uuid) async throws CatalogError -> Product {
        throw .notFound
    }
}

component ProductSkeleton {
    view {
        text { "Loading…" }
    }
}

component NotFound {
    view {
        text { "Not found" }
    }
}

component RetryPanel {
    @Emits let retry: Event<Void>

    view {
        button { "Retry" }
            .on(click: { retry.emit() })
    }
}
```
-->

```k
~import k.crypto.Uuid
~
page ProductDetail {
    @Param let id: Uuid
    @Inject let products: ProductService

    @Load let product: Product = try await products.find(id)

    metadata {
        title("{product.name} · Acme Shop")
        description(product.summary)
        canonical(.productDetail(id: id))
        socialPreview(image: product.imageUrl, title: product.name)
        structuredData(.product(name: product.name, price: product.price))
    }

    view {
        // …
~        heading(level: 1) { product.name }
    }

    loading { ProductSkeleton() }
}

layout Shop {
    @Slot let content: View

    metadata {
        title("Acme Shop")              // the default for every page in this layout
        icon(Url("https://acme.example/icon.svg"))
    }

    view {
        // …
~        main { content }
    }
}
```

## Rules

- **Entries are typed.** There are no raw `<meta name=…>` strings.
- **Metadata is reactive and rendered on the server**, so `@Load` values appear in the initial HTML for
  search engines and link previews.
- **Layouts provide defaults; pages override them.** For each entry, the innermost declaration wins.
- **`canonical` takes a typed page**, so canonical URLs come from `routes.lock` and can't drift. It
  renders the absolute URL, built from the app's public origin.
- **Stylesheets, scripts and fonts never appear in metadata.** The compiler emits CSS from `style` blocks,
  there is no third-party JavaScript, and theme `font` tokens become CSS with a fallback stack
  ([Fonts](styles.md#fonts)).

The compiler warns about a page with no `title` (`K3027`), and about a description longer than 160
characters (`K3028`):

- **A page has a title when any declaration it renders under sets one:** its own `metadata`, the layout
  it renders in (its `@Layout`, or the app's, none with `@NoLayout`), or the app's `metadata`. A `title`
  inside `failed` is the failure's own and doesn't count. A program without an `app` renders no document
  and gets no warning, and endpoint pages have no metadata. The quick fix adds `title("…")` made from the
  page's name (`OrderHistory` gets `title("Order history")`).
- **Only literal descriptions are measured,** in characters, in every `metadata` block. An interpolated
  description's length is known only at run time.

## Status codes

`status(_:)` sets the HTTP status of a server-rendered page. It is allowed only in a `metadata` block
inside `failed`, so a missing product answers `404` instead of a soft `200`:

```k
~import k.crypto.Uuid
~
~page Missing {
~    @Param let id: Uuid
~    @Inject let products: ProductService
~
~    @Load let product: Product = try await products.find(id)
~
~    view {
~        text { product.name }
~    }
~
~    loading { ProductSkeleton() }
failed(let error) {
    match error {
        case .notFound {
            metadata {
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
~}
```

## Metadata when a load fails

**When `failed` renders, the page-level `metadata` block is not evaluated.** It usually reads `@Load`
values, which don't exist when a load failed:

```k
~import k.crypto.Uuid
~
page ProductDetail {
    @Param let id: Uuid
~    @Inject let products: ProductService
    @Load let product: Product = try await products.find(id)

    metadata {
        title("{product.name} · Acme Shop")       // not evaluated when failed renders
    }

    view {
        // …
~        heading(level: 1) { product.name }
    }

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

What applies instead is the `metadata` block inside `failed`, if there is one, and the defaults from
`App.k` and the enclosing layouts, with the usual innermost-wins rule. So the `default` arm above, which
declares no `metadata`, shows the layout's title.

## Pages only

- **Metadata belongs to view pages.** [Endpoint pages](endpoints.md) have no `metadata`, and app and
  layout defaults don't apply to them.
- **Every response whose rendering read `Session` carries `Cache-Control: private, no-store`,** so a
  shared cache never serves one user's page to another.

## The locale

**The `locale` in effect is the only locale.** Number, date and money formatting use the app's or page's
`metadata` locale on the server, in the browser and in background jobs alike, never the browser's
language setting, so server-rendered text and hydrated text agree.

## App-wide entries

App-wide entries, such as the web manifest, the theme color and the default locale, go in `App.k`:

```k
// examples/weather/App.k
public app Weather {
    metadata {
        manifest(name: "Weather", shortName: "Weather")
        themeColor(.primary)
        locale(.english)
    }
}
```

See [The app declaration](app.md). Layout and page metadata override the app's, innermost first.
