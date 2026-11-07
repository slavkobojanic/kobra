/// The state of a value that is loaded asynchronously by hand, for code that
/// doesn't use [`Load`] (`docs/design/08-types.md`).
public enum LoadState<T> {
    /// Nothing has been requested yet.
    case idle
    /// A request is in flight.
    case loading
    /// The value arrived; the payload is the value.
    case loaded(T)
    /// The request failed; the payload is the error.
    case failed(Error)
}
