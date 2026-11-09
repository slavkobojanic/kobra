import k.auth.Session
import k.rpc.Channel
import k.rpc.RpcOnlyError

/// The server side of `HallProtocol`.
public service HallApi implements HallProtocol {
    @Inject let presence: HallPresence
    @Inject let session: Session

    rpc hall(name: String) async throws RpcOnlyError -> Channel<HallCommand, HallEvent, RpcOnlyError> {
        // Watch first, so nobody arriving between the two is missed.
        let live = presence.watch(hall: name).map { change => HallEvent.presence(change) }
        let me = session.user.id.name
        let lease = presence.join(hall: name, member: Member(name: me, status: "here"))
        let present = await presence.members(hall: name)

        return Channel(sending: live.startingWith(HallEvent.snapshot(present: present))) { command =>
            match command {
                case .status(let text) {
                    lease.update(member: Member(name: me, status: text))
                }

                case .leave {
                    lease.leave()
                }
            }
        }
    }

    rpc present(name: String) async throws RpcOnlyError -> List<Member> {
        return await presence.members(hall: name)
    }

    rpc peek(name: String) async throws RpcOnlyError -> List<Member> {
        presence.join(hall: name, member: Member(name: "{session.user.id.name} (peeking)", status: "here"))

        return await presence.members(hall: name)
    }
}
