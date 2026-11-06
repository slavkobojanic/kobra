// Interfaces may require fields and computed properties.
interface Named {
    let name: String
    var displayName: String { name }
    static let kind: String
}
