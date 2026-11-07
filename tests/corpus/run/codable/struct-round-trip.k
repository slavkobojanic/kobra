// Encoding a struct to JSON and decoding it back gives an equal value.
import k.encoding.Json
import k.http.DecodingError

struct Address implements Codable, Equatable {
    let city: String
    let lines: List<String>
}

struct Customer implements Codable, Equatable {
    let name: String
    let age: Int
    let vip: Bool
    let rating: Double
    let nickname: String?
    let address: Address
    let tags: Map<String, Int>
}

func main() throws DecodingError -> List<Bool> {
    let ana = Customer(name: "Ana", age: 30, vip: true, rating: 4.5, nickname: nil, address: Address(city: "Oslo", lines: ["Gate 1"]), tags: ["a": 1])
    let text = Json.encode(ana)
    let back: Customer = try Json.decode(text)
    return [back == ana, back.address.city == "Oslo", back.nickname == nil]
}
