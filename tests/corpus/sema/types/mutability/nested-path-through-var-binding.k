// Through a `var` root and `var` fields, nested assignment is fine.
struct Address {
    var city: String
}

struct User {
    var address: Address
}

func relocate(user: User) -> String {
    var copy = user
    copy.address.city = "Oslo"
    return copy.address.city
                      //^^^^ resolves: member corpus.Address.city
}
