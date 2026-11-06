import store.ProductSummary
import store.StoreService

/// The storefront: every product, in a grid.
///
/// `Home` has a named URL because it is the site root, which a URL derived
/// from the page name (`/home`) can't express.
public page Home at "/" {
    @Inject let store: StoreService

    /// Every product, ordered by name. On the server the call is in-process
    /// and can't fail; after client navigation the browser may be offline.
    @Load let products: List<ProductSummary> = try await store.products()

    metadata {
        title("K Shop · Small-batch goods for people who write K")
        description("Mugs, notebooks and stickers for K developers, made in small batches and shipped worldwide.")
        canonical(.home)
    }

    view {
        heading(level: 1) { "New this season" }

        if products.isEmpty {
            paragraph { "The shelves are empty. Check back soon." }
                .class(.empty)
        } else {
            box {
                for product in products key product.id {
                    ProductCard(product: product)
                }
            }
            .class(.grid)
        }
    }

    loading {
        paragraph { "Loading products…" }
            .class(.empty)
    }

    failed(let error) {
        match error {
            case .rpcFailure {
                paragraph { "The products couldn't be loaded. Check your connection and try again." }
                    .class(.empty)

                button { "Try again" }
                    .on(click: reload)
            }
        }
    }

    style {
        .grid {
            display: grid;
            gridTemplateColumns: repeat(auto-fill, minmax(14rem, 1fr));
            gap: .lg;
            marginTop: .lg;
        }

        .empty { color: .muted; }
    }
}
