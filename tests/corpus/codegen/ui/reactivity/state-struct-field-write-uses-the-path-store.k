// A write to a field of a `@State` struct goes through the path store, so only readers of that path
// update (runtime.md §3.3, codegen.md §6.3).
struct Address {
    var city: String
}

struct User {
    var name: String
    var address: Address
}

component Profile {
    @State var user: User = User(name: "Ada", address: Address(city: "London"))

    func move() {
        user.address.city = "Oslo"
    }

    view {
        box {
            text { user.name }

            text { user.address.city }

            button { "Move" }
                .on(click: move)
        }
    }
}
