# Proposal: Structure of `App.k`

> **Status: proposal.** Not yet accepted into the design. First used by `examples/weather`; extended by `examples/todos`.

## Problem

The design refers to `App.k` several times (DI choices per environment in [10-services.md](../10-services.md), layouts for groups of pages in [12-routing.md](../12-routing.md), interceptors in [11-protocols-and-rpc.md](../11-protocols-and-rpc.md), app-wide metadata in [22-metadata.md](../22-metadata.md)), and the README lists its structure as not yet covered.

## Proposal

`App.k` sits at the project root next to `k.toml` and holds exactly one declaration, introduced by a new **`app`** keyword:

```k
import layouts.Dashboard

/// A weather dashboard for any city.
@Layout(Dashboard)
public app Weather {
    use StripePayments for PaymentProvider in .production
    use FakePayments for PaymentProvider in .development, .test

    metadata {
        manifest(name: "Weather", shortName: "Weather")
        themeColor(.primary)
        locale(.english)
    }
}
```

### Rules

- **`app` is a declaration keyword**, because an app has its own compiler semantics: it is the root of the dependency graph and of the route table. There is exactly one per project. A second one, or an `app` outside `App.k`, is a compile error.
- **Nothing is registered by hand.** Pages, services, the `theme` and protocol implementations are found from their declarations. `App.k` contains only choices the compiler can't infer.
- **Members allowed in the body:**
  - `use X for Y [in .environment, …]` rules, as in [10-services.md](../10-services.md). The environments are `.development` (`k dev`), `.test` (`k test`) and `.production` (`k build`).
  - One `metadata { }` block with app-wide entries (`manifest`, `themeColor`, `locale`) plus defaults for any page entry. Layout and page metadata override it, innermost first.
  - Interceptors that never reject a call (logging, tracing). Their syntax is left to the interceptor design.
- **Annotations on `app` set app-wide defaults.** `@Layout(Dashboard)` wraps every page in `Dashboard` unless the page declares its own `@Layout`. A page that wants no layout declares `@Layout(.none)`.
  - `@SignInPage(SignIn)` names the page the `Authenticated` guard redirects to. It is required as soon as any page `requires Authenticated` (see [auth-and-sessions.md](auth-and-sessions.md); *extension first used by `examples/notes` and `examples/shop`*).
- **Grouping pages under different layouts** uses `@Layout` on the pages themselves. A per-group form in `App.k` is deferred until an example needs it.

### Rendering mode

*Extension first used by `examples/todos`.*

`@Rendering(.client)` on the `app` turns server rendering off for every page. The default, `@Rendering(.server)`, is the behavior described in [14-async-data.md](../14-async-data.md): pages render on the server and hydrate in the browser.

```k
@Rendering(.client)
public app Todos {
    metadata { … }
}
```

- **Use it when pages render from data only the browser has,** such as local storage ([local-storage.md](local-storage.md)). A server-rendered page would show an empty state and then jump to the real data.
- With client rendering, the server sends one HTML shell holding the app-wide `metadata`. Pages render in the browser, and page `metadata` is applied there.
- **When an app renders on the client and has no server-only code** (no `service … implements Protocol`, no repositories, no `@Scope(.request)`), `k build` emits only static files in `dist/public`, which any static host can serve. There is no server executable. Adding server-only code later brings `dist/app` back with no other change.
- It's an annotation on `app`, not a `k.toml` key, because it changes what the compiled program does, and `k.toml` holds no tool or build settings ([k-toml.md](k-toml.md)).
- Per-page `@Rendering` is left open: mixing modes raises hydration and navigation questions that no example has needed yet.

### Choosing an implementation per environment

*Extension first used by `examples/shop`.*

```k
@Layout(Storefront)
@SignInPage(SignIn)
public app Shop {
    use StripeCheckout for PaymentGateway in .production
    use LocalCheckout for PaymentGateway in .development, .test
}
```

- **The `use … in` rules for one interface must cover every environment** (`.development`, `.test`, `.production`), or one rule must have no `in` clause. A gap is a compile error naming the uncovered environment.
- **Environment differences are expressed only this way, and in configuration.** There is no `Environment.current` to branch on in code, so each environment's behavior can be read from `App.k`.
- Which configs a build requires follows from these rules: a config injected only by `StripeCheckout` is required only in production ([config-and-secrets.md](config-and-secrets.md)).
- `@SignInPage(SignIn)` is defined in [auth-and-sessions.md](auth-and-sessions.md).

## Why

- A keyword rather than a magic file name keeps "keywords declare kinds" true: the compiler finds the app by its declaration, like everything else.
- An annotation for the default layout reuses the existing `@Layout` and reads the same on an app as on a page.
- Keeping registration out of `App.k` means it stays small and never becomes a merge-conflict hotspot.

## Open questions

- ~~Secrets and environment configuration need a separate proposal.~~ Now [config-and-secrets.md](config-and-secrets.md): `config` declarations are server-only injectables read from the environment. `App.k` can't bind a config to a value, so no secret can be committed here.
- Security defaults (CSRF, CORS, content security policy, including which external hosts client code may `fetch`) would most naturally be configured here.
