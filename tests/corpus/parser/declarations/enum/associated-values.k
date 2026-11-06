// Cases may carry labeled associated values; the enum may implement interfaces.
enum CartError implements Error {
    case productNotFound
    case outOfStock(available: Int)
    case limitExceeded(limit: Int, requested: Int)
}
