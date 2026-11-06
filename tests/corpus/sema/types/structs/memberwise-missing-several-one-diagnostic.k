// Several missing fields are reported in one K2008, not one per field.
struct Address {
    let street: String
    let city: String
    let zip: String
    var country: String = "NO"
}

let home = Address(street: "Main 1")
