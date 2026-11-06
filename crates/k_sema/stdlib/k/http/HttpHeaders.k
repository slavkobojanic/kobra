/// The headers of a received request or response. Names are lowercased, so
/// lookups don't depend on the sender's capitalization: `headers["x-id"]`.
@Native
public struct HttpHeaders {
    /// The header's value, its values joined with `, ` if repeated, or `nil`.
    func get(_ name: String) -> String?
    /// Every value of the header, in order.
    func all(_ name: String) -> List<String>
}
