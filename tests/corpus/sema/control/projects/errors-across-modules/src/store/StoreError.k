/// Why a reservation failed.
public enum StoreError implements Error {
    case soldOut
    case missing(id: Int)
}
