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
- **A layout has no guards.** The server checks the `requires` clause of the page it answers, never of the layout the page renders in, so `layout Admin requires Authenticated` would protect nothing; it is a compile error (`K5014`) that asks for the clause on each page. Guarding every page of a layout through the layout would make a page's protection depend on an annotation far from it, and on the app's default layout, so it isn't done ([26-auth-and-sessions.md](26-auth-and-sessions.md#guards)).

## Navigation

View pages are cases of a **`Page` enum that the compiler generates**. This avoids name clashes (`page Product` vs `struct Product`). Navigation uses implicit member syntax and the named-or-chained rule:

```k
link(to: .product(id: product.id)) { product.name }

Router.navigate(to: .product(id: product.id).tab(.reviews))
```

- Navigating to a page that doesn't exist, or with missing or wrongly typed parameters, is a compile error.
- Every `Page` value has `url: Url`, its path and query (`/product/42?tab=reviews`). Absolute URLs are built by `PublicUrls` (below), on the server and in the browser.
- **External links and navigation:** `link(url: "https://…")` links out, and `Router.navigate(url: Url)` leaves the app with a full page load, for example to a hosted payment page.
- **Pages are always referenced as `Page` values:** in `link(to:)`, `Router.navigate(to:)`, `canonical(…)`, `@SignInPage(.signIn)` and `Page`-typed `@Param`s alike.
- **Navigating to the page already shown, with other parameters, updates it in place:** `.cityWeather(placeId: id).units(.imperial)` from `.cityWeather(placeId: id)` changes the `units` parameter of the mounted page, whose state stays, and re-runs only the loads that read it ([14-async-data.md](14-async-data.md#rules)).
- **The focus and the scroll after a navigation are the router's:** showing another page focuses its first level-1 heading and starts it at the top (or at its URL's `#fragment`); Back and Forward put each page where it was scrolled when it was left, and a reload keeps the position; a page updated in place keeps both ([53-element-references.md](53-element-references.md#focus-the-framework-moves)). As built in `runtime/router.js` (runtime.md §10, "Focus and scroll after a navigation"): `history.scrollRestoration` is `"manual"`, and each entry the router pushes carries its index, under which the position is kept in memory and, before a push and on `pagehide`, in the entry's state. A page still `loading` gets its position before its content, so a position below its `loading` view is clamped; restoring once the loads resolve is left for an app that needs it.

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
- In the browser the router checks `Authenticated` and `Role` against the `Session` mirror before showing a page. A page they refuse is loaded from the server instead, which answers as above. The same happens when the mirror loses the user while a guarded page is shown ([26-auth-and-sessions.md](26-auth-and-sessions.md#in-the-browser)).

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

`routes.lock` is TOML, written by `k build` and committed:

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

- **`[pages.<case>]` and `[endpoints.<case>]`** are keyed by the `Page` or `Endpoint` case and record the declaration's qualified name and the URL template. **`[redirects]`** maps an old URL template to the case of the page that answers it now; the server answers it with `308 Permanent Redirect`, building the new URL from the old one's parameters.
- **A rename is detected, not declared.** A recorded page counts as renamed when no page has its declaration or case any more, and exactly one new page of the same module (one the lock doesn't record) has the same path parameters, matching no other recorded page. Its old URL gets the automatic redirect, and redirects to the old case follow the new one. Anything else is a removal.
- **`k build` writes the lock:** every current page and endpoint, the recorded redirects (retargeted after renames, without those a page now serves), a redirect for each detected rename, and the entries of removed URLs, which stay until they are resolved. The server answers the automatic redirects even before the lock is rewritten.
- **`k check` compares the lock with the pages** (`K8007`): a recorded URL that nothing serves, with no redirect and no detected rename, is an error, and so is a detected rename whose redirect isn't recorded yet (run `k build` and commit the lock), a redirect to a page that no longer exists, and a redirect whose old URL lacks a parameter the page's URL needs. A page whose URL changed because its required parameters changed is a removal of the old URL. New pages need no entry until the next build.
- **Removing a page on purpose** means adding a redirect for its URL by hand, naming the URL with `at` on the page that should answer it, or deleting its entry once no link to it needs to work. An endpoint's URL is called by systems outside the app, so a removed endpoint is never redirected: keep its `at`, or delete the entry once nothing calls it.

## Endpoint pages

**A page that has HTTP handlers (`@Get`, `@Post`, …) instead of a `view` is an endpoint page.** It answers raw HTTP and exists only for external callers that can't speak Kobra's RPC: webhooks, OAuth callbacks, health checks. Everything else, including every call from the app's own client, is RPC ([11-protocols-and-rpc.md](11-protocols-and-rpc.md)), and developers otherwise never see HTTP.

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

Code that hands a URL to another system (a payment provider's return URL, a link in an email) needs it absolute. **`PublicUrls`** is a stdlib service in `k.server` that builds them from the public origin (`K_PUBLIC_ORIGIN`, [38-deployment.md](38-deployment.md)):

```k
service StoreApi implements StoreProtocol {
    @Inject let urls: PublicUrls

    ...
    let returnUrl = urls.absolute(.receipt(id: order.id))        // https://shop.example/receipt/0b6f…
    let webhookUrl = urls.absolute(endpoint: .stripeWebhook)    // https://shop.example/webhooks/stripe
}
```

- `absolute(_ page: Page) -> Url` and `absolute(endpoint: Endpoint) -> Url`. Developers still never write URLs.
- **`absolute(_ page:)` works in the browser too.** There it is the page's URL against `location.origin`, which is the public origin, so a client-rendered app builds a link to copy or share the same type-safe way (kanban's "Copy link"). It is no longer `@ServerOnly`; injecting it no longer makes a service server-only. `absolute(endpoint:)` stays server-only like endpoint pages: the browser never holds `Endpoint$href`, and a service that builds endpoint URLs is server-only (it is `@ServerOnly`, or injects a config or a repository, as real ones do). A call of `absolute(endpoint:)` in code that may run in the browser (a shared service, whose methods run wherever it is injected, or a component, page or layout) is `K4007` at the call (`di.rs`, `endpoint_urls_in_shared_code`; corpus `pages/endpoints/endpoint-url-in-shared-service.k`).
- `canonical(.page)` in `metadata` renders the absolute URL ([22-metadata.md](22-metadata.md)).
