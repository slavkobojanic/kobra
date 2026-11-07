/// A product: a struct return type is the response message itself.
public struct Product implements Codable {
    let slug: String
    let name: String
    let priceCents: Int
    let tags: List<String>
    let note: String?
}
