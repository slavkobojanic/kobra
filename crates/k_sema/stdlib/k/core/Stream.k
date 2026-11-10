/// An asynchronous sequence that is read once, with `for await`
/// (`docs/design/29-server-streaming.md`).
@Native
public struct Stream<T> {
    /// Returns a stream of the elements for which `predicate` is `true`,
    /// computed lazily as they arrive, one at a time and in order.
    /// `predicate` may await (a database read); closing the stream cancels
    /// the call running.
    func filter(_ predicate: (T) async -> Bool) -> Stream<T>
    /// Returns a stream of the results of `transform`, computed lazily,
    /// one at a time and in order. `transform` may await; closing the
    /// stream cancels the call running.
    func map<U>(_ transform: (T) async -> U) -> Stream<U>
    /// Returns a stream of the elements up to the first for which
    /// `predicate` is `false`, then ends: that element is dropped, and
    /// this stream is closed, so a `Broadcast` subscription is released.
    /// `predicate` may await.
    func prefix(while predicate: (T) async -> Bool) -> Stream<T>
    /// Returns a stream of the elements up to and including the first for
    /// which `predicate` is `true`, then ends: that element is yielded,
    /// and this stream is closed. A channel's `sending` stream ends so,
    /// which closes the channel `byServer` after its last message
    /// (`docs/design/44-channels.md`). `predicate` may await.
    func prefix(through predicate: (T) async -> Bool) -> Stream<T>
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
