/// An asynchronous sequence that is read once, with `for await`
/// (`docs/design/29-server-streaming.md`).
@Native
public struct Stream<T> {
    /// Returns a stream of the elements for which `predicate` is `true`,
    /// computed lazily as they arrive.
    func filter(_ predicate: (T) -> Bool) -> Stream<T>
    /// Returns a stream of the results of `transform`, computed lazily.
    func map<U>(_ transform: (T) -> U) -> Stream<U>
    /// Returns a stream that yields `value` first, then the elements of
    /// this one.
    func startingWith(_ value: T) -> Stream<T>
    /// Returns a stream that yields the elements of `first` and `second`
    /// as they arrive, and ends once both have ended: a channel's
    /// `sending` stream from several broadcasts
    /// (`docs/design/44-channels.md`).
    static func merge(_ first: Stream<T>, _ second: Stream<T>) -> Stream<T>
    /// Waits for the next element.
    ///
    /// - Returns: the element, or `nil` once the stream has ended.
    func next() async -> T?
}
