// All loads of a declaration throw the same error type, which `failed` binds.
enum PriceError implements Error, Codable {
    case missing
}

enum StockError implements Error, Codable {
    case missing
}

func price() async throws PriceError -> Int {
    return 1
}

func stock() async throws StockError -> Int {
    return 1
}

page Product {
    @Load let price: Int = try await price()
    @Load let stock: Int = try await stock()

    view {
        text { "{price} {stock}" }
    }

    loading {
    }
}
