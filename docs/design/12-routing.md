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
- **Layouts:** `@Layout(Shop)` on a page attaches a layout. `@Layout` on the `app` sets the default for every view page, and `@NoLayout` on a page opts out of it ([23-app-structure.md](23-app-structure.md)). `@Layout` always takes a layout's type name.

## Navigation

View pages are cases of a **`Page` enum that the compiler generates**. This avoids name clashes (`page Product` vs `struct Product`). Navigation uses implicit member syntax and the named-or-chained rule:

```k
link(to: .product(id: product.id)) { product.name }

Router.navigate(to: .product(id: product.id).tab(.reviews))
```

- Navigating to a page that doesn't exist, or with missing or wrongly typed parameters, is a compile error.
- Every `Page` value has `url: Url`, its path and query (`/product/42?tab=reviews`). Absolute URLs are built on the server by `PublicUrls` (below).
- **External links and navigation:** `link(url: "https://…")` links out, and `Router.navigate(url: Url)` leaves the app with a full page load, for example to a hosted payment page.
- **Pages are always referenced as `Page` values:** in `link(to:)`, `Router.navigate(to:)`, `canonical(…)`, `@SignInPage(.signIn)` and `Page`-typed `@Param`s alike.

## Parameters

- There is only `@Param`. The developer never chooses between path and query.
- Parameters are parsed into their types. An invalid value (for example, an `id` that isn't a valid `ProductId`) gives a 404.

## Guards: `requires`

```k
page Admin requires Authenticated, Role(.admin) { ... }
```

- Guards are the same middleware types used on protocols (see [11-protocols-and-rpc.md](11-protocols-and-rpc.md)), so `Authenticated` is one type everywhere.
- `requires` reads as prose. The word `validation` is reserved for form validation.
- A refused `Authenticated` redirects to the app's sign-in page with `303 See Other`, and a refused `Role` answers `403`. See [26-auth-and-sessions.md](26-auth-and-sessions.md#what-a-refused-page-does).

## URLs

### Default URLs

URLs are derived from the page name and its parameters:

| Declaration | URL |
|---|---|
| `page Pricing` | `/pricing` |
| `page SignIn` | `/sign-in` |
| `page Product` with required `id` | `/product/42` |
| …with optional `tab` set to `.reviews` | `/product/42?tab=reviews` |
| …with `tab` left at its default | `/product/42` |

- **Multi-word page names become kebab case:** `SignIn` → `/sign-in`, `ProductDetail` → `/product-detail`.
- **Required parameters become path segments,** in declaration order.
- **Optional parameters go in the query string,** and are omitted when equal to their default. Putting optional parameters in the path is ambiguous when there is more than one (`/product/42//newest`).

### Named URLs

When something outside the app depends on a URL (SEO, webhooks, OAuth callbacks, `/.well-known/*`), the page names it with `at`:

```k
page Pricing at "/pricing" { ... }

page ProductDetail at "/product/{id}/{tab}" { ... }     // optional parameters in the path, explicitly
```

Named URLs never change when the page is renamed.

### `routes.lock` and redirects

- The compiler records every page's URL in **`routes.lock`**, endpoint pages included.
- **Renaming an unnamed page changes its URL, and an automatic permanent redirect from the old URL is added.** URLs always match the current names, and old links keep working.
- Duplicate or conflicting URLs are compile errors.
- `k routes` prints the full route table, and the language server shows each page's URL inline.

## Endpoint pages

**A page that has HTTP handlers (`@Get`, `@Post`, …) instead of a `view` is an endpoint page.** It answers raw HTTP and exists only for external callers that can't speak K's RPC: webhooks, OAuth callbacks, health checks. Everything else, including every call from the app's own client, is RPC ([11-protocols-and-rpc.md](11-protocols-and-rpc.md)), and developers otherwise never see HTTP.

```k
page StripeWebhook at "/webhooks/stripe" {
    @Post
    func receive(request: IncomingRequest) async -> HttpResponse { ... }
}
```

- **An endpoint page must have a named URL.** Without `at`, it is a compile error with a quick fix that inserts `at "/<kebab-name>"`, because an external system depends on the URL and a derived one would move when the page is renamed.
- **Endpoint pages are not `Page` cases.** They are cases of a separate, generated, server-only **`Endpoint`** enum (`Endpoint.stripeWebhook.url`). So `link(to:)`, `Router.navigate(to:)`, `@SignInPage` and `Page`-typed parameters can't name one, by type, and the client bundle never contains them.
- Handlers, requests, responses, sessions and tests are specified in [27-endpoint-pages.md](27-endpoint-pages.md).

## Absolute URLs: `PublicUrls`

Code that hands a URL to another system (a payment provider's return URL, a link in an email) needs it absolute. **`PublicUrls`** is a server-only stdlib service in `k.server` that builds them from the public origin (`K_PUBLIC_ORIGIN`, [38-deployment.md](38-deployment.md)):

```k
service StoreApi implements StoreProtocol {
    @Inject let urls: PublicUrls

    ...
    let returnUrl = urls.absolute(.receipt(id: order.id))        // https://shop.example/receipt/0b6f…
    let webhookUrl = urls.absolute(endpoint: .stripeWebhook)    // https://shop.example/webhooks/stripe
}
```

- `absolute(_ page: Page) -> Url` and `absolute(endpoint: Endpoint) -> Url`. Developers still never write URLs.
- `canonical(.page)` in `metadata` renders the absolute URL ([22-metadata.md](22-metadata.md)).
