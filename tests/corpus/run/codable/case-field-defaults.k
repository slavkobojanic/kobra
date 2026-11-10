// A case's payload field with a default may be left out: when the case is
// built, and when a message from a sender built before the field was added
// is decoded.
import k.encoding.Json
import k.http.DecodingError

enum Event implements Codable, Equatable {
    case posted(room: String, number: Int, edited: Bool = false, by: String = "someone")
    case read(upTo: Int)
}

func describe(_ e: Event) -> String {
    return match e {
        case .posted(let room, let number, let edited, let by) { "{room} {number} {edited} {by}" }
        case .read(let upTo) { "read {upTo}" }
    }
}

func main() throws DecodingError -> List<String> {
    let old: Event = try Json.decode("{{\"case\": \"posted\", \"room\": \"general\", \"number\": 3}")
    let new: Event = try Json.decode("{{\"case\": \"posted\", \"room\": \"general\", \"number\": 4, \"edited\": true, \"by\": \"ana\"}")
    let built = Event.posted(room: "help", number: 1)
    let labelled = Event.posted(room: "help", number: 2, by: "bob")
    return [describe(old), describe(new), describe(built), describe(labelled), Json.encode(built), "{built == Event.posted(room: "help", number: 1, edited: false)}"]
}
