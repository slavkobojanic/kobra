// Inside `failed`, `status(_:)` still belongs in a `metadata` block.

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

    view {
        text { product.name }
    }

    loading {
    }

    failed {
        status(.notFound)
        text { "Not found" }
    }
}
