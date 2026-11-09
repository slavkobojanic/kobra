/// Comparisons whose time doesn't reveal where values differ.
@Native
public class ConstantTime {
    /// Compares `a` and `b` in time that depends only on their lengths,
    /// for protocol code holding plain bytes, such as a token's hash read
    /// from a database.
    ///
    /// - Returns: `true` if they are equal.
    static func equals(_ a: Bytes, _ b: Bytes) -> Bool
}
