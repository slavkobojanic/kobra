/// Fans values out to subscribers by key, across requests and replicas
/// (`docs/design/29-server-streaming.md`). Held by a `@Scope(.server)`
/// service ([`k.core.ServiceScope.server`]).
@ServerOnly
@Native
public class Broadcast<Key, T> where Key implements Hashable, Key implements Codable, T implements Codable {
    /// Creates a broadcast with no subscribers.
    @Native
    init() {}

    /// Delivers `value` to every current subscriber of `key`; inside a
    /// transaction, delivery waits for the app-level commit.
    func publish(key: Key, value: T)
    /// Subscribes to the values published for `key`.
    ///
    /// - Returns: a [`k.core.Stream`] that starts receiving immediately
    ///   and buffers values until they are read.
    func subscribe(key: Key) -> Stream<T>
}
