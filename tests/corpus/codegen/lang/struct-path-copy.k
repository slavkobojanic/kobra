// Assigning a nested field of a struct `var` is a path copy: one new object per level, no mutation
// (docs/compiler/codegen.md §6.3).
struct Address {
    var city: String
}

struct User {
    var name: String
    var address: Address
}

func moved(user: User) -> User {
    var u = user
    u.address.city = "Oslo"
    return u
}
