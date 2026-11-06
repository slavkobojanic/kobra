// Enums derive `Codable` from their payloads; one non-codable payload blocks it.
struct Plain {
    let x: Int
}

enum Good implements Codable {
    case none
    case some(value: Int, label: String)
}

enum Bad implements Codable {
    case none
    case wrapped(Plain)
}
