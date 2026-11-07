/// The outcome of a throwing call as a value: `Result.of { try f() }`
/// (`docs/design/09-errors.md`).
public enum Result<T, E> implements Codable where E implements Error {
    /// The call returned; the payload is its value.
    case success(T)
    /// The call threw; the payload is its error.
    case failure(E)

    /// Runs `block` and captures its value or error.
    ///
    /// - Parameter block: the throwing call to run.
    /// - Returns: [`success`] with the value `block` returned, or
    ///   [`failure`] with the error it threw.
    @Native
    static func of(_ block: () throws E -> T) -> Result<T, E>
}
