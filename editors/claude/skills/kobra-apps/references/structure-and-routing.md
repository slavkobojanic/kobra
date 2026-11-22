# App structure, routing and layouts

Sources: `book/getting-started/project-layout.md`, `book/reference/project-files.md`,
`book/apps/app.md`, `routing.md` and `metadata.md`, and `design/12-routing.md`, `23-app-structure.md`
and `24-projects.md`.

## `k.toml`

```toml
[package]
name = "weather"                   # required: lower case, digits, hyphens; the app's identity
title = "Weather"                  # display name (optional)
description = "A weather dashboard."
k = "0.1"                          # required: the toolchain (Cargo-style: any 0.1.x)
version = "0.1.0"                  # apps only, optional
```

`k.toml` holds no environment variables, no secrets, no per-environment settings and no build or tool
options. `[dev.env]` and `[dev.secrets]` give development-only values
(`read_doc book/reference/project-files.md`). `preview = ["workers"]` opts into preview features.
Git package `[dependencies]` are **Planned**. A workspace member depends on a sibling library with
`[dependencies] pay-contracts = { workspace = true }`.

## `App.k`

It holds exactly one `app`, and only the choices the compiler can't infer:

```k
import pay.LocalCheckout
import pay.PaymentGateway
import pay.StripeCheckout
import shop.ShopLayout

/// Skills Check: the app-wide settings.
@Layout(ShopLayout)
@SignInPage(.signIn)
public app SkillsCheck {
    use StripeCheckout for PaymentGateway in .production
    use LocalCheckout for PaymentGateway in .development, .test

    metadata {
        manifest(name: "Skills Check", shortName: "Skills")
        themeColor(.primary)
        locale(.english)
    }
}
```

- There are three environments: `.development` (`k dev`, `k run`), `.test` (`k test`) and
  `.production` (`k build`). The `use` rules for one interface must cover all three, or have one rule
  with no `in`. An interface with exactly one implementation needs no rule.
- `@Layout(Name)` sets the default layout of view pages, and `@SignInPage(.page)` is required once any
  page `requires Authenticated`.
- `@Rendering(.client)` turns server rendering off for the app. An app that is client-rendered and has
  no server-only code builds to static files only.
- `@Offline` (as in `examples/todos`) adds a service worker.

## Pages

A page is a component the router can show. It may have `view`, `style`, `metadata`, `@Load` with
`loading`/`failed`, `@State`, `@Effect` and `@Inject` (of services).

```k
import k.auth.Authenticated
import k.crypto.Uuid

enum OrderTab implements Equatable, Codable {
    case summary
    case receipt
}

/// One order. Its URL is `/order-detail/<id>?tab=receipt`.
public page OrderDetail requires Authenticated {
    @Param let id: Uuid
    @Param let tab: OrderTab = .summary

    metadata {
        title("Order")
    }

    view {
        heading(level: 1) { "Order {id}" }

        link(to: .orderDetail(id: id).tab(.receipt)) { "Receipt" }
    }
}
```

- **`@Param` is the only kind of parameter.** Required ones (no default) become path segments, in
  order. Optional ones go in the query string, and are left out when they equal their default.
  Parameter types are `String`, `Int`, `Uuid`, `Page`, payload-free enums, and optionals of these
  (K5002 otherwise). A custom id struct as a parameter is **Planned**.
- Default URLs: `page Pricing` is `/pricing` and `page SignIn` is `/sign-in` (kebab case).
  `page Home at "/"` names a URL explicitly, and named URLs never change on a rename:
  `page ProductDetail at "/product/{id}/{tab}"`.
- **Navigation is by the generated `Page` enum:** `link(to: .orders)`,
  `Router.navigate(to: .orderDetail(id: id))`. Optional parameters are chained (`.tab(.receipt)`).
  `Router` needs no import. Leave the app with `link(url: Url("https://…"))` or
  `Router.navigate(url:)`.
- Every view page needs a `title` from its `metadata`, its layout or the app (the K3027 warning fails
  `k check`). `metadata { }` also takes `description(…)`, `canonical(…)` and more
  (`read_doc book/apps/metadata.md`). In a `failed` block, `metadata { status(.notFound) }` sets the
  HTTP status.
- `routes.lock` records every URL. Renaming a page without `at` adds an automatic redirect, and `k
  build` records it (`k check` fails with K8007 until you do). `k routes` prints the table.
- A page and a type in the same module need different names.

## Layouts

```k
/// The frame around every page.
public layout ShopLayout {
    @Inject let auth: AuthService

    @Slot let content: View

    @State var problem: String? = nil

    func signOut() async {
        do {
            try await auth.signOut()

            Router.navigate(to: .signIn)
        } catch .rpcFailure {
            problem = "Couldn't reach the shop, so you're still signed in."
        }
    }

    view {
        header {
            link(to: .orders) { "Shop" }

            if auth.user != nil {
                button { "Sign out" }
                    .on(click: signOut)
            }
        }

        if let problem {
            paragraph { problem }
                .role(.alert)
        }

        main { content }
    }
}
```

`@Layout(ShopLayout)` on a page picks a layout, and `@NoLayout` opts out of the app default. Layouts
take no `requires` (K5014): guard each page instead.

## Guards on pages

`page Admin requires Authenticated, Role(.admin) { … }`

- When `Authenticated` refuses, the visitor is redirected (303) to the `@SignInPage`. If that page
  declares `@Param let returnTo: Page = .home`, the redirect fills it in.
- When `Role` refuses, the response is 403. `Role` needs the app's one `enum … implements AuthRole`.
- The browser checks the same guards before showing a page.

## Absolute URLs

`@Inject let urls: PublicUrls` (`k.server`) gives `urls.absolute(.receipt(id: order.id))` and,
server-side only, `urls.absolute(endpoint: .stripeWebhook)`. They're built from `K_PUBLIC_ORIGIN`.
