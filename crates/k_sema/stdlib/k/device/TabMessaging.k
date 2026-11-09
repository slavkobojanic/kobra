/// Messages between the app's tabs (`docs/design/45-web-platform.md`,
/// "Messages between tabs"). A client-only platform interface:
/// [`BrowserTabMessaging`] is injected with no `use` rule, and
/// [`k.test.TestTabMessaging`] replaces it in tests.
///
/// A message reaches the app's other tabs on the same origin and in the
/// same browser profile, never the tab that posted it. Tabs running
/// different builds of the app don't hear each other, since their message
/// types may differ.
@Platform(side: .client)
public interface TabMessaging {
    /// Sends `message` to the app's other tabs listening on `channel`.
    func post<T>(on channel: TabChannel<T>, message: T) where T implements Codable

    /// Calls `handler` with every message another tab posts on `channel`,
    /// until the returned subscription is cancelled. An `.app` service may
    /// subscribe in `init()` and drop it, since it lives as long as the tab.
    func listen<T>(on channel: TabChannel<T>, _ handler: (T) -> Void) -> Subscription where T implements Codable

    /// The messages other tabs post on `channel`, for an effect to follow
    /// with `for await`. The stream ends when its owner is cancelled.
    func messages<T>(on channel: TabChannel<T>) -> Stream<T> where T implements Codable
}
