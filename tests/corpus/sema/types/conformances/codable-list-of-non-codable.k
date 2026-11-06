// Conformance looks through generic arguments: `List<Plain>` is not `Codable` when `Plain` isn't.
struct Plain {
    let x: Int
}

struct Wrapper implements Codable {
    let items: List<Plain>
}
