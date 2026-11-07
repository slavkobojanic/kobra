// `bind user.address.city` creates the pair over the path store, so only readers of that path
// update (codegen.md §10, runtime.md §3.3).
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
            .label("City")
    }
}
