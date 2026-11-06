// Entity fields have column types; maps, plain structs and secrets are not.
struct Address {
    let city: String
}

entity Account {
    @Id let id: Int
    var settings: Map<String, String>
    var address: Address
    var token: Secret<String>
    var name: String
}
