# Pages and routing

Routes are declared in code, not derived from the file system, and **developers don't write URLs.**
Pages are typed values, and the compiler generates their URLs.

<!-- The declarations the samples in this chapter use; checked, not rendered.

```k
// src/store/Context.k
import k.auth.AuthRole
import k.crypto.Uuid
import k.http.HttpResponse
import k.http.IncomingRequest
import k.rpc.RpcOnlyError

enum ProductTab implements Equatable, Codable {
    case overview
    case reviews
}

struct CatalogItem {
    let id: Uuid
    let name: String
}

service ProductService {
    func all() -> List<CatalogItem> {
        return []
    }
}

struct Order {
    let id: Uuid
}

protocol StoreProtocol {
    rpc checkout(orderId: Uuid) throws RpcOnlyError -> Url
}

page Receipt {
    @Param let id: Uuid

    view {
        text { "Receipt {id}" }
    }
}

page StripeWebhook at "/webhooks/stripe" {
    @Post
    func receive(request: IncomingRequest) async -> HttpResponse {
        return HttpResponse(status: .ok)
    }
}

enum StaffRole implements AuthRole {
    case admin
}

component NavBar {
    view {
        nav { text { "K Shop" } }
    }
}

component Footer {
    view {
        footer { text { "© K Shop" } }
    }
}

service Readiness {
    func isReady() async -> Bool {
        return true
    }
}
```
-->

```k
~import k.crypto.Uuid
~import k.auth.Authenticated
page Product requires Authenticated {
    @Param let id: Uuid                    // required
    @Param let tab: ProductTab = .overview      // optional

    @Inject let products: ProductService

    view {
        // …
~        text { "Product" }
    }
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
- Parameters are parsed into their types. An invalid value, such as an `id` that isn't a valid `Uuid`,
  gives a `404`.
- **A URL can carry** `String`, `Int`, `Uuid`, `Page`, an enum whose cases have no payloads, and an
  optional of any of these. Any other type is `K5002`.

> **Planned.** The design also lets an id type of your own, such as a `ProductId` struct, be a
> parameter. The compiler doesn't accept one yet; use `Uuid` or `Int`.

## Navigation

View pages are cases of a **`Page` enum that the compiler generates**, so a link names `.product`, a
case, rather than the declaration itself. (A page and a type in the same module still need different
names.) You navigate with implicit member syntax, following the
named-or-chained rule: required parameters are arguments, optional ones are chained.

```k
~import k.crypto.Uuid
~import k.ui.Router
~
~page Product {
~    @Param let id: Uuid
~    @Param let tab: ProductTab = .overview
~
~    view {
~        text { "Product" }
~    }
~}
~
~component ProductLink {
~    @Prop let product: CatalogItem
~
~    view {
link(to: .product(id: product.id)) { product.name }
~    }
~
~    func open() {
Router.navigate(to: .product(id: product.id).tab(.reviews))
~    }
~}
```

- Navigating to a page that doesn't exist, or with missing or wrongly typed parameters, is a compile
  error.
- **Pages are always referenced as `Page` values:** in `link(to:)`, `Router.navigate(to:)`,
  `canonical(…)`, `@SignInPage(.signIn)` and `Page`-typed `@Param`s alike.
- Every `Page` value has `url: Url`, its path and query (`/product/42?tab=reviews`).
- **Leaving the app:** `link(url: Url("https://…"))` links out, and `Router.navigate(url: Url)` leaves with a
  full page load, for example to a hosted payment page.

## Absolute URLs: `PublicUrls`

Code that hands a URL to another system, such as a payment provider's return URL or a link in an email,
needs it absolute. **`PublicUrls`** is a server-only standard-library service in `k.server` that builds
absolute URLs from the app's public origin (`K_PUBLIC_ORIGIN`, see [Deployment](deployment.md)):

```k
~import k.rpc.RpcOnlyError
~import k.crypto.Uuid
~import k.server.PublicUrls
service StoreApi implements StoreProtocol {
    @Inject let urls: PublicUrls

    // …
~    rpc checkout(orderId: Uuid) async throws RpcOnlyError -> Url {
~        let order = Order(id: orderId)
    let returnUrl = urls.absolute(.receipt(id: order.id))        // https://shop.example/receipt/0b6f…
    let webhookUrl = urls.absolute(endpoint: .stripeWebhook)    // https://shop.example/webhooks/stripe
~        return returnUrl
~    }
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
~layout Shop {
~    @Slot let content: View
~
~    view {
~        main { content }
~    }
~}
~
@Layout(Shop)
page Pricing {
    // …
~    view { text { "Pricing" } }
}

@NoLayout
page Confirmation {
    // …
~    view { text { "Thanks" } }
}
```

## Guards: `requires`

```k
~import k.auth.Authenticated
~import k.auth.Role
page Admin requires Authenticated, Role(.admin) {
    // …
~    view { text { "Admin" } }
}
```

- Guards are the same middleware types used on [protocols](protocols.md), so `Authenticated` is one type
  everywhere.
- `requires` reads as prose. The word `validation` is reserved for form validation.
- When `Authenticated` refuses a page, the visitor is redirected with `303 See Other` to the app's
  sign-in page, named once with `@SignInPage(.signIn)` on the `app`. If the sign-in page declares
  `@Param let returnTo: Page`, the redirect fills it in, so the visitor comes back after signing in.
- `Role` refusing a page responds with `403 Forbidden`.
- The browser checks the same guards before it shows a page. A page they refuse is loaded from the
  server, which answers as above (see [What a refused page does](auth.md#what-a-refused-page-does)).
- Inside a page that `requires Authenticated`, including its `@Load` initializers, `session.user` is
  known to be present and needs no unwrapping.
- **Layouts take no guards.** The server checks the guards of the page it answers, never of its layout,
  so `layout Admin requires Authenticated` is a compile error (`K5014`): put the clause on each page that
  renders in the layout.

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
~import k.crypto.Uuid
page Home at "/" {
    // …
~    view { text { "Home" } }
}

page Pricing at "/pricing" {
    // …
~    view { text { "Pricing" } }
}

page ProductDetail at "/product/{id}/{tab}" {           // optional parameters in the path, explicitly
    @Param let id: Uuid
    @Param let tab: ProductTab = .overview
    // …
~    view { text { "Product" } }
}
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

`k build` writes `routes.lock`; `k check` compares the pages with it. It is TOML:

```toml
# Generated by `k build`. Add a redirect, or delete the entry of a URL
# that no longer needs to work; nothing else is edited by hand.
# Format: docs/design/12-routing.md

[pages.pricing]
declaration = "marketing.Pricing"
url = "/pricing"

[endpoints.stripeWebhook]
declaration = "payments.StripeWebhook"
url = "/webhooks/stripe"

[redirects]
"/plans" = "pricing"
```

- **A rename is detected.** When no page has a recorded page's declaration or case any more, and exactly
  one new page of the same module has the same path parameters, the compiler takes it as renamed: the
  server redirects the old URL to the new one with `308 Permanent Redirect`, carrying the parameters and
  the query, and `k build` records the redirect. Until it is recorded, `k check` fails (`K8007`) asking for
  `k build`, so the redirect is committed.
- **A URL nothing serves any more fails `k check`** (`K8007`): a removed page, or a page whose URL changed
  because its required parameters did. Keep the old URL with `at "/old"`, add a redirect under
  `[redirects]` to the page that should answer it, or delete the entry once no link to it needs to work.
  `k build` keeps such entries, so the check keeps failing until you decide.
- **An endpoint's URL is called by systems outside the app,** so it is never redirected: keep its `at`, or
  delete its entry once nothing calls it.
- `k check` also fails on a redirect to a page that no longer exists, and on one whose old URL lacks a
  parameter the page's URL needs.
- `k routes` lists every redirect, recorded or automatic, with the page it goes to.

## Endpoint pages

A page with HTTP handler functions instead of a `view` is an **endpoint page**: K's one deliberate
escape hatch to raw HTTP, for external systems that can't speak RPC, such as webhooks, OAuth callbacks and
health checks. Everything else, including every call from the app's own client, is RPC.

```k
~import k.http.HttpResponse
~import k.http.IncomingRequest
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
