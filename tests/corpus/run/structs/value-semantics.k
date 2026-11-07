// Structs have value semantics: changing a copy never changes the original
// (docs/design/08-types.md), including nested fields (a path copy).
struct Address {
    var city: String
}

struct User {
    var name: String
    var address: Address
}

func main() -> List<String> {
    let original = User(name: "Ana", address: Address(city: "Oslo"))
    var copy = original
    copy.name = "Ben"
    copy.address.city = "Bergen"
    return [original.name, original.address.city, copy.name, copy.address.city]
}
