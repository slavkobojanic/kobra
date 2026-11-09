import k.device.Subscription
import k.device.TabChannel
import k.device.TabMessaging

/// The default [`TabMessaging`] in `k test`
/// (`docs/design/45-web-platform.md`, "Messages between tabs"): there are
/// no other tabs. What this tab posts is recorded, and [`deliver`] plays
/// another tab posting.
@TestDefault(replaces: TabMessaging)
@Native
public service TestTabMessaging implements TabMessaging {
    /// Records `message` in [`posted(on:)`]; no listener of this tab hears
    /// it, as in the browser.
    func post<T>(on channel: TabChannel<T>, message: T) where T implements Codable
    /// Calls `handler` with each message [`deliver`] plays on `channel`.
    func listen<T>(on channel: TabChannel<T>, _ handler: (T) -> Void) -> Subscription where T implements Codable
    /// The messages [`deliver`] plays on `channel`; the stream ends with
    /// its owner.
    func messages<T>(on channel: TabChannel<T>) -> Stream<T> where T implements Codable

    /// Plays another tab posting `message` on `channel`: every listener and
    /// stream of this tab gets it, encoded and decoded as it would cross
    /// between tabs.
    func deliver<T>(on channel: TabChannel<T>, message: T) where T implements Codable
    /// What this tab posted on `channel`, oldest first.
    func posted<T>(on channel: TabChannel<T>) -> List<T> where T implements Codable
}
