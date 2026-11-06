# Pages and routing

Routes are declared in code, not derived from the file system, and **developers don't write URLs.**
Pages are typed values, and the compiler generates their URLs.

```k
page Product requires Authenticated {
    @Param let id: ProductId                    // required
    @Param let tab: ProductTab = .overview      // optional

    @Inject let products: ProductService

    view { … }
}
```

- `page` and `layout` are declaration keywords.
- A page is a component that the router can show. It has a `view`, and may have `style`, `metadata`,
  `@Load` fields with `loading` and `failed` blocks, state and effects.
- Pages and layouts follow the same injection rules as components: they inject services only.

## Parameters

There is only one kind of parameter, `@Param`. **You never choose between path and query parameters**;
the compiler decides, as described under [URLs](#urls).

- A parameter without a default is required. One with a default is optional.
- Parameters are parsed into their types. An invalid value, such as an `id` that isn't a valid
  `ProductId` or `Uuid`, gives a `404`.

## Navigation

View pages are cases of a **`Page` enum that the compiler generates**, which avoids name clashes between
`page Product` and `struct Product`. You navigate with implicit member syntax, following the
named-or-chained rule: required parameters are arguments, optional ones are chained.

```k
link(to: .product(id: product.id)) { product.name }

Router.navigate(to: .product(id: product.id).tab(.reviews))
```

- Navigating to a page that doesn't exist, or with missing or wrongly typed parameters, is a compile
  error.
- **Pages are always referenced as `Page` values:** in `link(to:)`, `Router.navigate(to:)`,
  `canonical(…)`, `@SignInPage(.signIn)` and `Page`-typed `@Param`s alike.
- Every `Page` value has `url: Url`, its path and query (`/product/42?tab=reviews`).
- **Leaving the app:** `link(url: "https://…")` links out, and `Router.navigate(url: Url)` leaves with a
  full page load, for example to a hosted payment page.

## Absolute URLs: `PublicUrls`

Code that hands a URL to another system, such as a payment provider's return URL or a link in an email,
needs it absolute. **`PublicUrls`** is a server-only standard-library service in `k.server` that builds
absolute URLs from the app's public origin (`K_PUBLIC_ORIGIN`, see [Deployment](deployment.md)):

```k
service StoreApi implements StoreProtocol {
    @Inject let urls: PublicUrls

    …
    let returnUrl = urls.absolute(.receipt(id: order.id))        // https://shop.example/receipt/0b6f…
    let webhookUrl = urls.absolute(endpoint: .stripeWebhook)    // https://shop.example/webhooks/stripe
}
```

`absolute(_ page: Page) -> Url` and `absolute(endpoint: Endpoint) -> Url` take typed pages, so you
still never write a URL by hand.

## Layouts

A layout wraps pages with shared UI. Its required slot receives the page:

```k
layout Shop {
    @Slot let content: View

    view {
        NavBar()

        main { content }

        Footer()
    }
}
```

- `@Layout(Shop)` on a page attaches a layout. `@Layout` always takes a layout's type name.
- `@Layout(Storefront)` on the `app` in `App.k` makes `Storefront` the default for every view page. A
  page can still choose its own with `@Layout`, or opt out with **`@NoLayout`**, which takes no argument.

```k
@Layout(Shop)
page Pricing { … }

@NoLayout
page Receipt { … }
```

## Guards: `requires`

```k
page Admin requires Authenticated, Role(.admin) { … }
```

- Guards are the same middleware types used on [protocols](protocols.md), so `Authenticated` is one type
  everywhere.
- `requires` reads as prose. The word `validation` is reserved for form validation.
- When `Authenticated` refuses a page, the visitor is redirected with `303 See Other` to the app's
  sign-in page, named once with `@SignInPage(.signIn)` on the `app`. If the sign-in page declares
  `@Param let returnTo: Page`, the redirect fills it in, so the visitor comes back after signing in.
- `Role` refusing a page responds with `403 Forbidden`.
- Inside a page that `requires Authenticated`, including its `@Load` initializers, `session.user` is
  known to be present and needs no unwrapping.

See [Authentication and sessions](auth.md).

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

- **Multi-word page names become kebab case:** `SignIn` is at `/sign-in`, `ProductDetail` at
  `/product-detail`.
- **Required parameters become path segments**, in declaration order.
- **Optional parameters go in the query string**, and are omitted when equal to their default. Optional
  parameters in the path would be ambiguous as soon as there is more than one (`/product/42//newest`).

### Named URLs

When something outside the app depends on a URL, such as search engines, webhooks, OAuth callbacks or
`/.well-known/*`, the page names it with `at`:

```k
page Home at "/" { … }

page Pricing at "/pricing" { … }

page ProductDetail at "/product/{id}/{tab}" { … }     // optional parameters in the path, explicitly
```

The URL is an ordinary string whose interpolations must each name a `@Param` of the page. **Named URLs
never change when the page is renamed.**

### `routes.lock` and redirects

- The compiler records every page's URL in **`routes.lock`**, which you commit. Endpoint pages are
  included.
- **Renaming a page without a named URL changes its URL, and an automatic permanent redirect from the old
  URL is added.** URLs always match the current names, and old links keep working.
- Duplicate or conflicting URLs are compile errors.
- `k routes` prints the full route table.

## Endpoint pages

A page with HTTP handler functions instead of a `view` is an **endpoint page**: K's one deliberate
escape hatch to raw HTTP, for external systems that can't speak RPC, such as webhooks, OAuth callbacks and
health checks. Everything else, including every call from the app's own client, is RPC.

```k
public page HealthCheck at "/health" {
    @Inject let readiness: Readiness

    @Get
    func check(request: IncomingRequest) async -> HttpResponse {
        guard await readiness.isReady() else {
            return HttpResponse(status: .serviceUnavailable)
                .body(.text("database unavailable"))
        }

        return HttpResponse(status: .ok)
            .body(.text("ok"))
    }
}
```

- **An endpoint page must have a named URL**, because an external system depends on it. Without `at`,
  it is a compile error with a quick fix that inserts one.
- **Endpoint pages are not `Page` cases.** They are cases of a separate, server-only, generated
  **`Endpoint`** enum, so `link(to:)`, `Router.navigate(to:)` and `@SignInPage` can't name one, by type.
  `Endpoint.healthCheck.url` is its path.

[Endpoint pages](endpoints.md) covers handlers, requests, responses and sessions.
