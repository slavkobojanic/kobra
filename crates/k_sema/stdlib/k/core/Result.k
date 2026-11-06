/// The outcome of a throwing call as a value: `Result.of { try f() }`
/// (`docs/design/09-errors.md`).
public enum Result<T, E> implements Codable where E implements Error {
    /// The call returned `value`.
    case success(T)
    /// The call threw `error`.
    case failure(E)

    /// Runs `block` and captures its value or error.
    @Native
    static func of(_ block: () throws E -> T) -> Result<T, E>
}
