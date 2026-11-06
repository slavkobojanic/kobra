/// A type a `config` field can have: its value is parsed from an
/// environment variable (`docs/design/25-config-and-secrets.md`).
///
/// Implemented by `String`, `Int`, `Double`, `Decimal`, `Bool`, `Duration`,
/// `Url`, `Uuid`, `EmailAddress`, enums without payloads, `List<T>` and
/// `Secret<T>`.
public interface ConfigValue {}
