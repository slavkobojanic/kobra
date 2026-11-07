// Enums decode from the same JSON they encode to; an unknown case name is a mismatch.
import k.encoding.Json
import k.http.DecodingError

enum Status implements Codable, Equatable {
    case active
    case paused(until: String)
}

func decode(_ text: String) -> String {
    do {
        let s: Status = try Json.decode(text)
        return match s {
            case .active { "active" }
            case .paused(let until) { "paused until {until}" }
        }
    } catch _ {
        return "error"
    }
}

func main() -> List<String> {
    let encoded = Json.encode(Status.paused(until: "monday"))
    return [decode("\"active\""), decode(encoded), decode("\"deleted\""), decode("{{\"case\": \"paused\"}")]
}
