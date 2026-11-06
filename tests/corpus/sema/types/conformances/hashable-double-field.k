// `Double` is not `Hashable`, so a struct with a `Double` field can't derive it.
struct Price implements Hashable {
    let amount: Double
}

struct Sku implements Hashable {
    let code: String
    let size: Int
}
