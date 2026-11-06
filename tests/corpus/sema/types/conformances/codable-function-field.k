// A function-typed field is not `Codable`: K2009 at `Codable` in the `implements` clause.
struct Handler implements Codable {
    let name: String
    let run: () -> Void
}
