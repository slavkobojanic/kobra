// `implements` is the only conformance syntax.
struct User implements Codable, Equatable, Hashable {
    let id: UserId
}
