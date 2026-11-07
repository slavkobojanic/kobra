/// The headers of a received request or response. Names are lowercased, so
/// lookups don't depend on the sender's capitalization: `headers["x-id"]`.
@Native
public struct HttpHeaders {
    /// Returns the value of the header `name`, matched in any case.
    ///
    /// - Returns: the value, the values joined with `, ` if the header is
    ///   repeated, or `nil` if it is absent.
    func get(_ name: String) -> String?
    /// Returns every value of the header `name`, in order; empty if it is
    /// absent.
    func all(_ name: String) -> List<String>
}
