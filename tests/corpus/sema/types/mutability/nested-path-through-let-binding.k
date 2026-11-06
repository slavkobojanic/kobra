// A deep path is immutable when its root binding is `let`; the diagnostic spans the whole place.
struct Address {
    var city: String
}

struct User {
    var address: Address
}

func relocate(user: User) -> String {
    let copy = user
    copy.address.city = "Oslo"
    return copy.address.city
}
