/// A type a `config` field can have: its value is parsed from an
/// environment variable (`docs/design/25-config-and-secrets.md`).
///
/// Implemented by [`String`], [`Int`], [`Double`], [`Decimal`], [`Bool`],
/// [`Duration`], [`Url`], [`k.crypto.Uuid`], [`k.text.EmailAddress`],
/// enums without payloads, [`List`] and [`Secret`].
public interface ConfigValue {}
