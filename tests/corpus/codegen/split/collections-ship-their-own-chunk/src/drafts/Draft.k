/// A draft.
public struct Draft implements Codable {
    let id: Int
    @JsonKey("saved_at")
    let savedAt: Date
}
