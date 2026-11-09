# Async Data

## Decision

Pages and components load async data with **`@Load` fields**, plus **`loading` and `failed` blocks** next to `view`.

```k
page Product {
    @Param let id: ProductId
    @Inject let products: ProductService

    @Load let product: Product = try await products.find(id)
    @Load let reviews: List<Review> = try await products.reviews(for: product.id)
    @Load let related: List<Product> = try await products.related(to: id)

    view {
        // product, reviews and related are plain, already-loaded values here
        heading(level: 1) { product.name }

        ReviewList(reviews: reviews)
    }

    loading {
        ProductSkeleton()
    }

    failed(let error) {
        match error {
            case .notFound {
                metadata {
                    status(.notFound)
                }

                NotFound()
            }

            case .network(let e) {
                RetryPanel(error: e)
                    .on(retry: reload)
            }
        }
    }
}
```

## Rules

- **`@Load` is an async `@Derived`.** It re-runs when its inputs change (for example, `id` on client navigation). The previous run is cancelled.
- **Only the loads whose inputs changed re-run,** with the loads that read them. On the weather dashboard, a `units` change re-runs `forecast` and keeps `place` and `airQuality`; a `placeId` change re-runs all three, because the other two read `place`. The page renders `loading` until the re-run loads resolve, as the table [below](#what-renders-during-a-re-run) says. A hydrated page watches the same inputs, so its first change after hydration re-runs just as narrowly.
- **A navigation to the same page with other parameters keeps the page mounted.** Its `@Param`s change in place, so its `@State` stays and only what reads a changed parameter updates or re-runs. A navigation to another page, or to the same page from a different set of layouts, mounts it afresh. (Both decided after the real-browser runs found a units change re-fetching every load.)
- **`view` renders only once every `@Load` field has resolved,** so values are unwrapped inside it. There are no `if let` checks and no `isLoading` flags.
- **The loading order comes from the dependency graph.** `reviews` reads `product`, so it waits for `product`. `related` only reads `id`, so it runs in parallel with `product`. There is no `Promise.all`.
- **`loading` is required whenever `@Load` is used.** An empty `loading { }` is the explicit way to render nothing, so a loading state is never forgotten.
- **`failed` receives the typed union of the errors the loads can throw.** `match` keeps it exhaustive. If `failed` is omitted, the error bubbles to the nearest `ErrorBoundary`.
- **Caching and deduplication belong to services** (see [10-services.md](10-services.md)). `@Load` only calls them.
- **Components can use `@Load` too.** Each component is then its own loading boundary, so a slow sidebar doesn't block the page.

## Reloading: `reload()`

Every page, layout or component that declares `@Load` fields gets a generated method **`reload()`**. It re-runs all of the declaration's loads in dependency order: a retry button in `failed`, or a periodic refresh ([35-time.md](35-time.md)).

- `reload()` is synchronous. It starts the re-run and returns. Any run still in flight is cancelled first, as for an input change.
- It re-runs every load, not a chosen subset. Loads that are cheap to repeat should be cached in their service.
- Declaring a member named `reload` in a declaration with `@Load` fields is a compile error.

### What renders during a re-run

| Situation | Renders |
|---|---|
| First load, nothing resolved yet | `loading` |
| An **input changed** (for example a different `@Param`) | `loading`, because the old values describe something else |
| **`reload()`** with every load previously resolved | `view`, with the previous values, until all new values resolve |
| `reload()` from `failed` | `loading` |
| Any re-run throws | `failed` |

**The swap is atomic:** during a reload, the view keeps every `@Load` value from the previous run until all loads have finished, and then all are replaced at once. Dependent values stay consistent with each other.

## Server rendering and hydration

- **Server rendering awaits the loads and serializes the results to the client,** so the same data is never fetched twice during hydration.
- **`@Load` values must be `Codable`,** because they are serialized. Any other type is a compile error at the field.
- **So must the errors they can throw.** A load that fails on the server renders `failed`, and hydrating it needs the error in the browser. Error types thrown by `@Load` initializers follow the rules for rpc error types ([28-rpc-wire-format.md](28-rpc-wire-format.md)): they must be `Codable`, and the compiler warns when a payload is a server-only type or an upstream response (such as an `HttpError` carrying a third party's response body), with a hint to wrap the error without it.
- **`status(_:)` sets the HTTP status of a server-rendered `failed` page.** It is allowed only in a `metadata { }` block inside `failed`, so a "not found" page answers `404` instead of a soft `200` for crawlers. During client navigation it has no effect.
- **When `failed` renders, the page's own `metadata` block isn't evaluated,** because it usually reads `@Load` values that don't exist (`product.name`). Only a `metadata` block inside `failed`, and the app's and layouts' defaults, apply ([22-metadata.md](22-metadata.md#metadata-when-a-load-fails)).
- Until hydration completes, the browser's `Clock` returns the server's render instant, so relative times don't differ between the server's HTML and hydration ([35-time.md](35-time.md#server-rendering)).

## Open questions

- Whether pages want an `isReloading: Bool` to show a subtle "refreshing" indicator.
- Whether a failed *reload* should keep the stale view and expose the error, instead of switching to `failed`. That would be friendlier for dashboards, but it adds a second error path to every page.
