// Codable enums: unlabelled payloads, several labels, and nested enums.
enum Color implements Codable {
    case red
    case custom(hex: String)
}

enum Paint implements Codable {
    case solid(Color)
    case gradient(from: Color, to: Color)
}

func main() -> List<Paint> {
    return [.solid(.red), .gradient(from: .red, to: .custom(hex: "#fff"))]
}
