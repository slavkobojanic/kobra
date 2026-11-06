# The app declaration

Every K application has an `App.k` at the project root, next to `k.toml`. It holds exactly one `app`
declaration.

> **Proposal.** The structure of `App.k` follows `docs/design/proposals/app-structure.md`. The design
> itself establishes that `App.k` holds per-environment `use` rules, layouts for groups of pages,
> interceptors and app-wide metadata.

```k
import layouts.Storefront
import accounts.SignIn

/// An online shop.
@Layout(Storefront)
@SignInPage(SignIn)
public app Shop {
    use StripeCheckout for PaymentGateway in .production
    use LocalCheckout for PaymentGateway in .development, .test

    metadata {
        manifest(name: "Acme Shop", shortName: "Shop")
        themeColor(.primary)
        locale(.english)
    }
}
```

## Nothing is registered by hand

Pages, services, the theme and protocol implementations are found from their declarations. You never
list your routes or register a service. **`App.k` contains only the choices the compiler can't infer**,
which keeps it short, and keeps it from becoming a merge-conflict hotspot.

## What goes in it

| Member | Purpose |
|---|---|
| `use X for Y [in .environment, …]` | which implementation of an interface to inject, optionally per environment |
| `metadata { … }` | app-wide document entries, and defaults for every page |
| interceptors | logging and tracing around every rpc (syntax not yet designed) |

| Annotation | Purpose |
|---|---|
| `@Layout(Name)` | the default layout for every page; a page can choose its own, or `@Layout(.none)` |
| `@SignInPage(Name)` | where `requires Authenticated` sends visitors who aren't signed in |
| `@Rendering(.client)` | turns off server rendering for the whole app; the default is `.server` |

## Environments

There are three environments, and each command builds one of them:

| Environment | Built by |
|---|---|
| `.development` | `k dev` |
| `.test` | `k test` |
| `.production` | `k build` |

`use … in` rules for one interface must cover all three, or one rule must have no `in` clause. A gap is a
compile error naming the uncovered environment.

**There is no `Environment.current` to branch on in code.** Every difference between environments is a
`use` rule here, or a configuration value read at runtime, so you can see exactly how environments differ
by reading `App.k`. See [Configuration and deployment](deployment.md).

## Rendering on the client

By default, pages render on the server and hydrate in the browser. An app whose pages render from data
only the browser has, such as local storage, turns server rendering off:

```k
@Rendering(.client)
public app Todos {
    metadata {
        manifest(name: "Todos", shortName: "Todos")
        themeColor(.primary)
        locale(.english)
    }
}
```

When such an app also has no server-only code, `k build` emits only static files in `dist/public`, which
any static host can serve.
