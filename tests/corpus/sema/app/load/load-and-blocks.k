// `@Load` fields render `view` once loaded; `loading` and `failed(let error)` cover the rest, and the
// error is the loads' error type. `reviews` reads `product`, so it waits for it.

struct Product implements Codable {
    let id: Int
    let name: String
}

struct Review implements Codable {
    let text: String
}

enum ProductError implements Error, Codable {
    case notFound
    case unavailable
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
        //                  ^^^^^^^ type: corpus.Product
        text { "{reviews.count} reviews" }
    }

    loading {
        text { "Loading…" }
    }

    failed(let error) {
    //         ^^^^^ type: corpus.ProductError
        match error {
            case .notFound {
                text { "Not found" }
            }
            case .unavailable {
                text { "Try again later" }
            }
        }
    }
}
