// Assigning a nested field changes only that `var`; a copy taken before keeps the old nested value.
struct Address {
    var city: String
    var zip: String
}

struct User {
    var name: String
    var address: Address
}

func main() -> List<String> {
    var user = User(name: "Ana", address: Address(city: "Oslo", zip: "0150"))
    let before = user
    user.address.city = "Bergen"
    let after = user
    user.address.zip = "5003"
    return [before.address.city, before.address.zip, after.address.city, after.address.zip, user.address.city, user.address.zip]
}
