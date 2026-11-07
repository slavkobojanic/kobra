// The `@Load` fields of a page are one load group: each load names the loads it reads in `after`,
// so `reviews` waits for `product` (14-async-data.md, runtime.md §7).
struct Product implements Codable {
    let id: Int
    let name: String
}

struct Review implements Codable {
    let text: String
}

enum ProductError implements Error, Codable {
    case notFound
}

service ProductService {
    func find(id: Int) async throws ProductError -> Product {
        return Product(id: id, name: "Lamp")
    }

    func reviews(productId: Int) async throws ProductError -> List<Review> {
        return []
    }
}

page ProductPage {
    @Param let id: Int
    @Inject let products: ProductService

    @Load let product: Product = try await products.find(id: id)
    @Load let reviews: List<Review> = try await products.reviews(productId: product.id)

    view {
        heading(level: 1) { product.name }

        text { "{reviews.count} reviews" }
    }

    loading {
        text { "Loading…" }
    }

    failed(let error) {
        text { "Not found" }
    }
}
