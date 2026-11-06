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
            case .notFound { NotFound() }

            case .network(let e) { RetryPanel(error: e) }
        }
    }
}
```

## Rules

- **`@Load` is an async `@Derived`.** It re-runs when its inputs change (for example, `id` on client navigation). The previous run is cancelled.
- **`view` renders only once every `@Load` field has resolved,** so values are unwrapped inside it. There are no `if let` checks and no `isLoading` flags.
- **The loading order comes from the dependency graph.** `reviews` reads `product`, so it waits for `product`. `related` only reads `id`, so it runs in parallel with `product`. There is no `Promise.all`.
- **`loading` is required whenever `@Load` is used.** An empty `loading { }` is the explicit way to render nothing, so a loading state is never forgotten.
- **`failed` receives the typed union of the errors the loads can throw.** `match` keeps it exhaustive. If `failed` is omitted, the error bubbles to the nearest `ErrorBoundary`.
- **Server rendering awaits the loads and serializes the results to the client,** so the same data is never fetched twice during hydration.
- **Caching and deduplication belong to services** (see [10-services.md](10-services.md)). `@Load` only calls them.
- **Components can use `@Load` too.** Each component is then its own loading boundary, so a slow sidebar doesn't block the page.
