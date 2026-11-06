// A field whose struct doesn't implement `Codable` blocks the derivation.
struct Plain {
    let x: Int
}

struct Wrapper implements Codable {
    let inner: Plain
}
