/// A type whose values can be thrown: `throws E` requires `E implements
/// Error` (`docs/design/09-errors.md`).
///
/// Error types are ordinary enums. An error that crosses the wire (an rpc
/// error, a [`Load`] error) must also be [`Codable`].
public interface Error {}
