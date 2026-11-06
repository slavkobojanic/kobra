/// Fans values out to subscribers by key, across requests and replicas
/// (`docs/design/29-server-streaming.md`). Held by a `@Scope(.server)`
/// service.
@ServerOnly
@Native
public class Broadcast<Key, T> where Key implements Hashable, Key implements Codable, T implements Codable {
    /// An empty broadcast.
    @Native
    init() {}

    /// Delivers `value` to every current subscriber of `key`; deferred to
    /// the app-level commit inside a transaction.
    func publish(key: Key, value: T)
    /// A subscription that starts immediately and buffers until read.
    func subscribe(key: Key) -> Stream<T>
}
