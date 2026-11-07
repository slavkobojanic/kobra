import k.rpc.RpcOnlyError

/// The server implementation of `CatalogProtocol`.
public service CatalogApi implements CatalogProtocol {
    @Inject let calls: HandlerCalls

    rpc ping() async throws RpcOnlyError {
        calls.record()
    }

    rpc echo(text: String) async throws RpcOnlyError -> String {
        calls.record()

        return text
    }

    rpc product(for slug: String) async throws CatalogError -> Product {
        calls.record()

        guard slug == "k-mug" else {
            throw CatalogError.productNotFound
        }

        return Product(slug: slug, name: "K <Mug>", priceCents: 1200, tags: ["mug", "k"], note: nil)
    }

    rpc byIndex(_ id: Int) async throws CatalogError -> Product {
        calls.record()

        guard id == 1 else {
            throw CatalogError.productNotFound
        }

        return Product(slug: "k-mug", name: "K <Mug>", priceCents: 1200, tags: [], note: "first")
    }

    rpc find(slug: String) async throws RpcOnlyError -> Product? {
        calls.record()

        if slug == "k-mug" {
            return Product(slug: slug, name: "K <Mug>", priceCents: 1200, tags: [], note: nil)
        }

        return nil
    }

    rpc availability(slug: String) async throws RpcOnlyError -> Availability {
        calls.record()

        return if slug == "k-mug" { .inStock } else { .soldOut }
    }

    rpc slugs() async throws RpcOnlyError -> List<String> {
        calls.record()

        return ["k-mug", "k-shirt"]
    }

    rpc reserve(slug: String, quantity: Int) async throws CatalogError -> Int {
        calls.record()

        guard quantity <= 3 else {
            throw CatalogError.outOfStock(available: 3)
        }

        return 3 - quantity
    }

    rpc reprice(slug: String) async throws CatalogError -> Int {
        calls.record()

        throw CatalogError.priceChanged(from: 1200, to: 1500, reason: "<tax>")
    }

    rpc addReview(review: ReviewDraft) async throws RpcOnlyError -> Int {
        calls.record()

        return review.stars
    }

    rpc handlerCalls() async throws RpcOnlyError -> Int {
        return calls.count
    }

    rpc crash() async throws RpcOnlyError -> Int {
        let empty: List<Int> = []

        return empty[1]
    }
}
