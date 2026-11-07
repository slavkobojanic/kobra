// `copy(...)` replaces the named fields, `let` fields included, and leaves the original unchanged.
struct User implements Copyable, Codable {
    let id: Int
    var name: String
    var email: String
}

func main() -> List<User> {
    let ana = User(id: 1, name: "Ana", email: "a@example.com")
    let renamed = ana.copy(name: "Anna")
    let same = ana.copy()
    let other = ana.copy(id: 2, email: "b@example.com")
    return [ana, renamed, same, other]
}
