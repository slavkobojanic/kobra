/// A type that can be encoded to and decoded from JSON and Protobuf.
///
/// Structs and enums derive it with `implements Codable` when every field and
/// payload is `Codable`. Rpc messages, `@Load` values, job payloads, storage
/// values and broadcast messages must be `Codable`
/// (`docs/design/08-types.md`, `docs/design/28-rpc-wire-format.md`).
public interface Codable {}
