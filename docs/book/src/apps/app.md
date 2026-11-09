# The app declaration

Every K application has an `App.k` at the project root, next to `k.toml`. It holds exactly one
declaration, introduced by the **`app`** keyword, and it contains only the choices the compiler can't
infer.

<!-- The declarations the sample below uses; checked, not rendered.

```k
// src/layouts/Storefront.k
/// The storefront's layout.
public layout Storefront {
    /// The page.
    @Slot let content: View

    view {
        main { content }
    }
}
```

```k
// src/payments/PaymentGateway.k
/// Starts checkouts.
public interface PaymentGateway {
    /// Returns where to send the customer to pay.
    func startCheckout(amount: Money) async -> Url
}
```

```k
// src/payments/StripeCheckout.k
/// Checkout through Stripe.
public service StripeCheckout implements PaymentGateway {
    func startCheckout(amount: Money) async -> Url {
        return Url("https://checkout.stripe.com")
    }
}
```

```k
// src/payments/LocalCheckout.k
/// A checkout that pays at once.
public service LocalCheckout implements PaymentGateway {
    func startCheckout(amount: Money) async -> Url {
        return Url("http://localhost:8080/receipt")
    }
}
```

```k
// src/accounts/SignIn.k
/// The sign-in page.
public page SignIn {
    view {
        text { "Sign in" }
    }
}
```
-->

```k
// App.k
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

`app` is a declaration keyword because an app has its own compiler semantics: it is the root of the
dependency graph and of the route table. There is exactly one per project. A second one, or an `app`
outside `App.k`, is a compile error.

## Nothing is registered by hand

Pages, services, the theme, configs, jobs and protocol implementations are found from their
declarations. You never list your routes or register a service. **`App.k` contains only the choices the
compiler can't infer**, which keeps it short, and keeps it from becoming a merge-conflict hotspot.

The sign-in page is named as a `Page` value, `.signIn`, like every other page reference, so `App.k`
needs no import for it.

## What goes in it

| Member | Purpose |
|---|---|
| `use X for Y [in .environment, …]` | which implementation of an interface to inject, optionally per environment |
| `metadata { … }` | app-wide entries (`manifest`, `themeColor`, `locale`), plus defaults for any page entry |
| interceptors | logging and tracing around every rpc; their syntax is still open |

`App.k` can't bind a [config](config.md) to a value, so no secret can ever be committed here.

### Annotations on `app`

Each annotation takes exactly one kind of argument:

| Annotation | Argument | Meaning |
|---|---|---|
| `@Layout(Dashboard)` | a layout's type name | wraps every view page in `Dashboard`, unless the page declares its own `@Layout`, or `@NoLayout` |
| `@SignInPage(.signIn)` | a `Page` value | where the `Authenticated` guard sends visitors who aren't signed in. Required as soon as any page `requires Authenticated`. |
| `@Rendering(.client)` | `.server` or `.client` | turns server rendering off for every page. The default is `.server`. On a component, renders only that component in the browser ([Components](components.md#rendering-in-the-browser-only)). |

- **`@NoLayout`** on a page opts out of the app's default layout. It takes no argument.
- **App-level `@Layout` and `metadata` apply only to view pages.** `@Layout` or `@NoLayout` on an
  [endpoint page](endpoints.md) is a compile error.
- The sign-in page can't itself `requires Authenticated`.

## Environments

There are three environments, and each command builds one of them:

| Environment | Built by |
|---|---|
| `.development` | `k dev` |
| `.test` | `k test` |
| `.production` | `k build` |

- **`use … in` rules for one interface must cover all three,** or one rule must have no `in` clause. A
  gap is a compile error naming the uncovered environment.
- **There is no `Environment.current` to branch on in code.** Every difference between environments is
  a `use` rule here, or a configuration value read at runtime, so you can see exactly how environments
  differ by reading `App.k`.
- **Which configs a build requires follows from these rules.** A config injected only by `StripeCheckout`
  is required only in production. See [Configuration and secrets](config.md).
- In tests, a test group's own `use` rules override the `.test` rules here.

## Rendering on the client

By default, pages render on the server and hydrate in the browser. An app whose pages render from data
only the browser has, such as [local storage](local-storage.md), turns server rendering off:

```k,standalone
/// A to-do list that keeps everything in the browser.
@Rendering(.client)
public app Todos {
    metadata {
        manifest(name: "Todos", shortName: "Todos")
        themeColor(.primary)
        locale(.english)
    }
}
```

- With client rendering, the server sends one HTML shell holding the app-wide `metadata`. Pages render in
  the browser, and page `metadata` is applied there.
- **The shell carries the session mirror,** and the server checks a page's guards before sending it, so
  `Session` and page guards work exactly as in a server-rendered app ([Authentication and
  sessions](auth.md)).
- In a server-rendered app, a page that reads a client-only service is a warning (`K4013`) with a quick fix
  that adds this annotation. When only part of a page needs the browser, put `@Rendering(.client)` on that
  component instead ([Components](components.md#rendering-in-the-browser-only)).
- **When an app renders on the client and has no server-only code** (no protocol implementations,
  repositories, configs, jobs, endpoint pages, guarded pages or `.request`/`.server` services), `k build` emits only
  static files in `dist/public`, which any static host can serve. Adding server-only code later brings
  `dist/app` back with no other change.
- It's an annotation on `app`, not a `k.toml` key, because it changes what the compiled program does, and
  `k.toml` holds no build settings.

Interceptor syntax and per-page rendering modes are deferred by the design; see
[Design status](../reference/design-status.md).
