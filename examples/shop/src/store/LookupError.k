/// The error for looking up a single product or order.
public enum LookupError implements Error {
    /// Nothing has the requested slug or id.
    case notFound
}
