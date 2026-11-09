/// The standard [`TabMessaging`]: the browser's `BroadcastChannel`, one per
/// channel name, namespaced per app as storage keys are.
@ClientOnly
@Native
public service BrowserTabMessaging implements TabMessaging {
    /// Posts to the other tabs; see [`TabMessaging.post`].
    func post<T>(on channel: TabChannel<T>, message: T) where T implements Codable
    /// Listens to the other tabs; see [`TabMessaging.listen`].
    func listen<T>(on channel: TabChannel<T>, _ handler: (T) -> Void) -> Subscription where T implements Codable
    /// Streams the other tabs' messages; see [`TabMessaging.messages`].
    func messages<T>(on channel: TabChannel<T>) -> Stream<T> where T implements Codable
}
