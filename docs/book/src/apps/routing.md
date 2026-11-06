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
  `ProductId`, gives a `404`.

## Navigation

Pages are cases of a **`Page` enum that the compiler generates**, which avoids name clashes between
`page Product` and `struct Product`. You navigate with implicit member syntax, following the
named-or-chained rule: required parameters are arguments, optional ones are chained.

```k
link(to: .product(id: product.id)) { product.name }

Router.navigate(to: .product(id: product.id).tab(.reviews))
```

- Navigating to a page that doesn't exist, or with missing or wrongly typed parameters, is a compile
  error.
- `link(url: "https://…")` is for external links.

> **Proposal.** `Router.navigate(url:)` for leaving the app, and `absoluteUrl` for URLs handed to other
> systems, follow `docs/design/proposals/deployment.md`.

Every `Page` value has a `url`, the path and query, and in server code an `absoluteUrl` that includes
the app's public origin. Code that gives a URL to another system, such as a payment provider's return URL
or a link in an email, uses `.absoluteUrl`, so you still never write a URL by hand.

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

A page attaches to a layout with `@Layout`:

```k
@Layout(Shop)
page Pricing { … }
```

> **Proposal.** The app-wide default layout follows `docs/design/proposals/app-structure.md`.

`@Layout(Shop)` on the `app` in `App.k` makes `Shop` the default for every page. A page can still choose
its own, or opt out with `@Layout(.none)`.

## Guards: `requires`

```k
page Admin requires Authenticated, Role(.admin) { … }
```

- Guards are the same middleware types used on [protocols](protocols.md), so `Authenticated` is one type
  everywhere.
- `requires` reads as prose. The word `validation` is reserved for form validation.

> **Proposal.** What a refused page does follows `docs/design/proposals/auth-and-sessions.md`.

When `Authenticated` refuses a page, the visitor is redirected to the app's sign-in page, named once with
`@SignInPage(SignIn)` on the `app`. If the sign-in page declares `@Param let returnTo: Page`, the redirect
fills it in, so the visitor comes back after signing in. `Role` refusing a page responds with `403
Forbidden`. Inside a page that `requires Authenticated`, `session.user` is known to be present and needs
no unwrapping.

## URLs

### Default URLs

URLs are derived from the page name and its parameters:

| Declaration | URL |
|---|---|
| `page Pricing` | `/pricing` |
| `page Product` with required `id` | `/product/42` |
| …with optional `tab` set to `.reviews` | `/product/42?tab=reviews` |
| …with `tab` left at its default | `/product/42` |

- **Required parameters become path segments**, in declaration order.
- **Optional parameters go in the query string**, and are omitted when equal to their default. Optional
  parameters in the path would be ambiguous as soon as there is more than one (`/product/42//newest`).
- Multi-word page names become kebab case: `page SignIn` is at `/sign-in`. (*Proposal:*
  `docs/design/proposals/deployment.md`.)

### Named URLs

When something outside the app depends on a URL, such as search engines, webhooks, OAuth callbacks or
`/.well-known/*`, the page names it with `at`:

```k
page Home at "/" { … }

page Pricing at "/pricing" { … }

page ProductDetail at "/product/{id}/{tab}" { … }     // optional parameters in the path, explicitly
```

Interpolations in the URL must each name a `@Param` of the page. **Named URLs never change when the page
is renamed.**

### `routes.lock` and redirects

- The compiler records every page's URL in **`routes.lock`**, which you commit.
- **Renaming a page without a named URL changes its URL, and an automatic permanent redirect from the old
  URL is added.** URLs always match the current names, and old links keep working.
- Duplicate or conflicting URLs are compile errors.
- `k routes` prints the full route table.

## Endpoint pages

> **Proposal.** Endpoint pages follow `docs/design/proposals/server-endpoints.md`.

A page with HTTP handler functions instead of a `view` is an **endpoint page**: a raw HTTP handler for
webhooks and health checks, answering with a status code instead of HTML.

```k
public page HealthCheck at "/health" {
    @Inject let readiness: Readiness

    @Get
    func check(request: HttpRequest) async -> HttpResponse {
        guard await readiness.isReady() else {
            return HttpResponse(status: 503, body: "database unavailable")
        }

        return HttpResponse(status: 200, body: "ok")
    }
}
```

- Handlers are annotated with their method (`@Get`, `@Post`, `@Put`, `@Patch`, `@Delete`) and have one
  fixed signature. They don't throw: every failure becomes a response the handler chose.
- Endpoint pages are server-only, and still cases of the `Page` enum, with named URLs and guards.
- `request.rawBody` holds the body exactly as received, for verifying webhook signatures, and header names
  are lowercased.
