// Bindings work on nested fields of a `@State` struct (`bind user.address.city`,
// 04-components.md).
struct Address {
    var city: String
}

struct User {
    var address: Address
}

component Profile {
    @State var user: User = User(address: Address(city: ""))

    view {
        textField(value: bind user.address.city)
        //                    ^^^^^^^^^^^^^^^^^ type: String
    }
}
