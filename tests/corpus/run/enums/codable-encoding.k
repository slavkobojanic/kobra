// Codable enums encode payload-free cases by name and payload cases with their labels
// (docs/design/28-rpc-wire-format.md).
enum Status implements Codable {
    case active
    case paused(until: String)
}

func main() -> List<Status> {
    return [.active, .paused(until: "monday")]
}
