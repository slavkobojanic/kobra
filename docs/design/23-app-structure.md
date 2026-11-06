# App Structure

## Decision

`App.k` sits at the project root next to `k.toml` and holds exactly one declaration, introduced by the **`app`** keyword. It contains only the choices the compiler can't infer.

```k
import layouts.Storefront
import payments.LocalCheckout
import payments.PaymentGateway
import payments.StripeCheckout

/// K Shop: a storefront with guest checkout and Stripe payments.
@Layout(Storefront)
@SignInPage(.signIn)
public app Shop {
    use StripeCheckout for PaymentGateway in .production
    use LocalCheckout for PaymentGateway in .development, .test

    metadata {
        manifest(name: "K Shop", shortName: "Shop")
        themeColor(.primary)
        locale(.english)
    }
}
```

## Rules

- **`app` is a declaration keyword**, because an app has its own compiler semantics: it is the root of the dependency graph and of the route table. There is exactly one per project. A second one, or an `app` outside `App.k`, is a compile error.
- **Nothing is registered by hand.** Pages, services, the `theme`, configs, jobs and protocol implementations are found from their declarations, so `App.k` stays small and never becomes a merge-conflict hotspot.
- **Members allowed in the body:**
  - `use X for Y [in .environment, …]` rules ([10-services.md](10-services.md#injecting-interfaces)). `App.k` can't bind a config to a value, so no secret can be committed here ([25-config-and-secrets.md](25-config-and-secrets.md)).
  - One `metadata { }` block with app-wide entries (`manifest`, `themeColor`, `locale`) plus defaults for any page entry. Layout and page metadata override it, innermost first ([22-metadata.md](22-metadata.md)).
  - Interceptors that never reject a call (logging, tracing). Their syntax is still open.

### Annotations on `app`

Each annotation takes exactly one kind of argument:

| Annotation | Argument | Meaning |
|---|---|---|
| `@Layout(Dashboard)` | a layout's type name | wraps every view page in `Dashboard` unless the page declares its own `@Layout`, or `@NoLayout` |
| `@SignInPage(.signIn)` | a `Page` value | the page the `Authenticated` guard redirects to ([26-auth-and-sessions.md](26-auth-and-sessions.md#what-a-refused-page-does)). Required as soon as any page `requires Authenticated`. |
| `@Rendering(.client)` | a `RenderingMode` (`.server` or `.client`) | turns server rendering off for every page (below). The default is `.server`. |

- **`@NoLayout`** on a page opts out of the app's default layout. It takes no argument.
- **App-level `@Layout` and `metadata` apply only to view pages.** `@Layout` or `@NoLayout` on an endpoint page is a compile error ([27-endpoint-pages.md](27-endpoint-pages.md)).
- The sign-in page can't itself `requires Authenticated`; that is a compile error.
- Grouping pages under different layouts uses `@Layout` on the pages themselves.

## Environments

The environments are `.development` (`k dev`), `.test` (`k test`) and `.production` (`k build`).

- **The `use … in` rules for one interface must cover every environment,** or one rule must have no `in` clause. A gap is a compile error naming the uncovered environment.
- **Environment differences are expressed only this way, and in configuration.** There is no `Environment.current` to branch on in code, so each environment's behavior can be read from `App.k`.
- **Which configs a build requires follows from these rules.** A config injected only by `StripeCheckout` is required only in production ([25-config-and-secrets.md](25-config-and-secrets.md#validated-at-startup-all-at-once)).
- In tests, a test group's own `use` rules override the `.test` rules here ([21-testing.md](21-testing.md#test-doubles-and-defaults)).

## Rendering mode

`@Rendering(.client)` on the `app` turns server rendering off for every page. The default, `@Rendering(.server)`, is the behavior described in [14-async-data.md](14-async-data.md): pages render on the server and hydrate in the browser.

```k
@Rendering(.client)
public app Todos {
    metadata { … }
}
```

- **Use it when pages render from data only the browser has,** such as local storage ([36-local-storage.md](36-local-storage.md)). In a server-rendered app, reading a client-only service from a page is a warning with a quick fix that adds this annotation ([10-services.md](10-services.md#serverclient-boundary)).
- With client rendering, the server sends one HTML shell holding the app-wide `metadata`. Pages render in the browser, and page `metadata` is applied there.
- **When an app renders on the client and has no server-only code** (no protocol implementations, repositories, configs, jobs, endpoint pages or `.request`/`.server` services), `k build` emits only static files in `dist/public`, which any static host can serve ([38-deployment.md](38-deployment.md#static-only-apps)). Adding server-only code later brings `dist/app` back with no other change.
- It's an annotation on `app`, not a `k.toml` key, because it changes what the compiled program does, and `k.toml` holds no tool or build settings ([24-projects.md](24-projects.md)).

## Why

- A keyword rather than a magic file name keeps "keywords declare kinds" true: the compiler finds the app by its declaration, like everything else.
- An annotation for the default layout reuses `@Layout` and reads the same on an app as on a page. A separate `@NoLayout` keeps `@Layout`'s argument a layout type and nothing else.
- Referencing the sign-in page as a `Page` value matches every other page reference.

## Open questions

- Interceptor syntax.
- Per-page `@Rendering`: mixing modes raises hydration and navigation questions that no example has needed yet.
- Security headers (content security policy, CORS for anything other than RPC, which never grants it) would most naturally be configured here.
