import k.auth.Session
import k.rpc.Channel
import k.rpc.RpcOnlyError

/// The server side of `LobbyProtocol`.
public service LobbyApi implements LobbyProtocol {
    @Inject let log: LobbyLog
    @Inject let session: Session

    rpc lobby() async throws RpcOnlyError -> Channel<LobbyCommand, LobbyEvent, CommandError> {
        let live = log.subscribe()

        return Channel(sending: live.startingWith(LobbyEvent.snapshot(lines: log.all()))) { command =>
            match command {
                case .say(let text) {
                    guard text != "" else {
                        throw CommandError.empty
                    }

                    log.add("{session.user.id.name}: {text}")
                }
            }
        }
    }

    rpc announce(text: String) async throws RpcOnlyError {
        log.add(text)
    }
}
