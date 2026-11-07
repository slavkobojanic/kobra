// `Json.encode` writes compact JSON with fields in declaration order.
import k.encoding.Json

struct Item implements Codable {
    let name: String
    let quantity: Int
    let note: String?
}

func main() -> String {
    return Json.encode(Item(name: "mug", quantity: 2, note: nil))
}
