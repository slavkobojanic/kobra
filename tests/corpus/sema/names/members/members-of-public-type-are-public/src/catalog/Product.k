// Members of a public type are public without a modifier: fields, functions and static members.
public struct Product {
    let name: String

    static let sample = Product(name: "Lamp")

    func label() -> String {
        return name.uppercased()
    }
}
