// `implements Codable` derives when every field is `Codable`: built-ins, optionals, lists, maps and other Codable structs.
struct Address implements Codable {
    let city: String
}

struct User implements Codable {
                     //^^^^^^^ resolves: decl Codable
    let id: Int
    var name: String?
    var tags: List<String>
    var scores: Map<String, Double>
    var address: Address
    var joined: Date
}
let user: User? = nil
        //^^^^ resolves: decl corpus.User
