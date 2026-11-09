/// Another browser in the same test ([`TestPeers`]): its own session,
/// cookie and channel socket, sharing the test's server, database and
/// clock.
@Native
public class TestPeer {
    /// A client of protocol `P`, the type the result is used as
    /// (`let chat: ChatProtocol = bob.client()`): its rpcs and channels
    /// run under this browser's session, on its own socket.
    func client<P>() -> P
    /// Signs this browser's person out, ending its session and the
    /// channels opened under it.
    func signOut() async
}
