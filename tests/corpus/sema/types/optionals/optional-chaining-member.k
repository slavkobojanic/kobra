// `x?.y` has type `Y?`; chains of `?.` stay one level optional.
struct Address {
    var city: String
}

struct User {
    var address: Address?
}

func city(user: User?) -> String? {
    let address = user?.address
      //^^^^^^^ type: corpus.Address?
    let city = user?.address?.city
      //^^^^ type: String?
                            //^^^^ resolves: member corpus.Address.city
    return city
}
