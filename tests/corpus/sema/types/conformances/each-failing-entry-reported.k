// Each derived conformance that fails is its own K2009; passing ones beside them are fine.
struct Plain {
    let x: Int
}

struct Mixed implements Codable, Equatable, Hashable {
    let plain: Plain
    let price: Double
}
