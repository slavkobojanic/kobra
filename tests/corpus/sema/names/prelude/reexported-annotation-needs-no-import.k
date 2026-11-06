// Re-exported annotations (`k.encoding.JsonKey`) are in the prelude; `k.encoding.Json` itself is not.
struct Payload implements Codable {
    @JsonKey("user_name") let userName: String
}
