// Assigning a field of a struct held in a service's state is a write to the service's field.
struct Address {
    var city: String
}

service Profile {
    @State var address: Address = Address(city: "Berlin")
}

component Move {
    @Inject let profile: Profile

    func move() {
        profile.address.city = "Paris"
    }

    view {
        button { "Move" }
            .on(click: move)
    }
}
