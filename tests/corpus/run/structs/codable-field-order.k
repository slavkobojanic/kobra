// A struct encodes its fields in declaration order with their K names; nil optionals are null.
struct Profile implements Codable {
    let name: String
    let age: Int
    let nickname: String?
    let scores: List<Double>
}

func main() -> Profile {
    return Profile(name: "Ana", age: 30, nickname: nil, scores: [1.5, 2.0])
}
