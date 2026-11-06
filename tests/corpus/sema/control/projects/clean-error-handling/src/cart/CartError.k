/// Why a cart operation failed.
public enum CartError implements Error {
    case productNotFound
    case outOfStock(available: Int)
}
