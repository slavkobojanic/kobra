# Async data loading

Pages and components load async data with **`@Load` fields**, and describe what to show meanwhile with a
**`loading` block** and, optionally, a **`failed` block** next to `view`.

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

        RelatedProducts(products: related)
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

## How it works

- **`@Load` is an async `@Derived`.** It re-runs when its inputs change, for example when `id` changes on
  client-side navigation, and the previous run is cancelled.
- **`view` renders only once every `@Load` field has resolved**, so inside it the values are plain
  values. There are no `if let` checks and no `isLoading` flags.
- **The loading order comes from the dependency graph.** `reviews` reads `product`, so it waits for
  `product`. `related` only reads `id`, so it runs in parallel with `product`. There is no `Promise.all`
  to write.

## `loading`

**`loading` is required whenever `@Load` is used.** A loading state is never forgotten. An empty
`loading { }` is the explicit way to render nothing.

## `failed`

`failed` receives **the typed union of the errors the loads can throw**, and `match` keeps it exhaustive.
`failed { … }` without a binding is allowed too. If `failed` is omitted, the error goes to the nearest
`ErrorBoundary`.

## Server rendering

- **Server rendering awaits the loads and serializes the results to the client**, so the same data is
  never fetched twice during hydration. A `@Load` field's type must therefore be `Codable`; any other type
  is a compile error at the field.
- **So must the errors the loads can throw.** A load that fails on the server renders `failed`, and
  hydrating it needs the error in the browser. Load errors follow the rules for rpc errors: they must be
  `Codable`, and the compiler warns when a payload is a server-only type or carries a third party's
  response body, such as an `HttpError.status` from an external API. Wrap such errors without the body,
  for example as `case upstream(HttpStatus)`.
- **`status(_:)` sets the HTTP status of a server-rendered `failed` page.** It is allowed only in a
  `metadata { }` block inside `failed`, so a "not found" page answers `404` instead of a soft `200` for
  crawlers. During client navigation it has no effect.
- The first client render reads the time from the server's render instant, so relative times such as
  "5 minutes ago" are the same in the server's HTML and after hydration.

## Caching belongs to services

`@Load` only calls services. Caching, deduplication and retries belong to the
[service](services.md), where every page that needs the data shares them.

## Components can load too

A component with `@Load` fields is its own loading boundary. A slow sidebar shows its own `loading`
state without holding up the page around it:

```k
component RecentOrders {
    @Inject let orders: OrderService

    @Load let recent: List<OrderSummary> = await orders.recent()

    view {
        list {
            for order in recent key order.id {
                item { OrderLine(order: order) }
            }
        }
    }

    loading {
        paragraph { "Loading orders…" }
    }
}
```

## Reloading

Every page, layout or component with `@Load` fields gets a generated `reload()` method, which re-runs all
of its loads in dependency order. Use it for retry buttons and periodic refreshes:

```k
failed(let error) {
    paragraph { "Something went wrong." }

    button { "Try again" }
        .on(click: reload)
}
```

| Situation | Shows |
|---|---|
| First load | `loading` |
| An input changed, such as a different `@Param` | `loading`, because the old values describe something else |
| `reload()` after a successful load | `view` with the previous values, until all new values arrive, then swaps them in at once |
| `reload()` from `failed` | `loading` |
| Any load throws | `failed` |

- `reload()` is synchronous. It starts the re-run and returns; any run still in flight is cancelled first.
- It re-runs every load, not a chosen subset. Loads that are cheap to repeat should be cached in their
  service.
- **The swap is atomic:** during a reload, the view keeps every `@Load` value from the previous run until
  all loads have finished, and then all are replaced at once, so dependent values stay consistent.
- Declaring your own member named `reload` next to `@Load` fields is a compile error.

To refresh periodically, start a timer in an effect and cancel it on cleanup:

```k
@Effect(on: [placeId])
func refreshPeriodically() {
    let timer = Timer.every(10.minutes) { reload() }

    onCleanup { timer.cancel() }
}
```

## When to use `LoadState` instead

`@Load` is for data a view needs before it can render. For data that a user action fetches, such as
search results, keep a `LoadState<T>` in `@State` and set it from an effect or a function:

```k
@State var results: LoadState<List<Place>> = .idle

func search() async {
    results = .loading
    results = .loaded(await places.search(query))
}
```

Then `match results` in the view.
