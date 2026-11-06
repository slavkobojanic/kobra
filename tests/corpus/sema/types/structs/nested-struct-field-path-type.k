// Nested struct fields are typed along the whole path.
struct Address {
    var city: String
}

struct User {
    let name: String
    var address: Address
}

func city(user: User) -> String {
    return user.address.city
         //^^^^^^^^^^^^ type: corpus.Address
         //^^^^^^^^^^^^^^^^^ type: String
}
