/// An asynchronous sequence that is read once, with `for await`
/// (`docs/design/29-server-streaming.md`).
@Native
public struct Stream<T> {
    /// The elements for which `predicate` is `true`, lazily.
    func filter(_ predicate: (T) -> Bool) -> Stream<T>
    /// The results of `transform`, lazily.
    func map<U>(_ transform: (T) -> U) -> Stream<U>
    /// A stream that yields `value` first, then the elements of this one.
    func startingWith(_ value: T) -> Stream<T>
    /// The next element, or `nil` once the stream has ended.
    func next() async -> T?
}
