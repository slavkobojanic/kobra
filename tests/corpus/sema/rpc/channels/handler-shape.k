// A channel handler repeats its rpc's `Channel<C, S, E>`, sends a `Stream<S>`, and its receiving function takes `C` and throws `E`.
import k.rpc.Channel
import k.rpc.RpcFailure
import k.rpc.RpcOnlyError

enum Command implements Codable {
    case post(text: String)
}

enum Event implements Codable {
    case posted(text: String)
}

enum CommandError implements Error, Codable {
    case notYours
    case rpcFailure(RpcFailure)
}

enum OtherError implements Error, Codable {
    case nope
    case rpcFailure(RpcFailure)
}

protocol Chat {
    rpc room(id: Int) throws RpcOnlyError -> Channel<Command, Event, CommandError>
    rpc lobby(id: Int) throws RpcOnlyError -> Channel<Command, Event, CommandError>
    rpc hall(id: Int) throws RpcOnlyError -> Channel<Command, Event, CommandError>
}

service ChatApi implements Chat {
    rpc room(id: Int) async throws RpcOnlyError -> Channel<Command, Command, CommandError> {
        return Channel(sending: commands()) { command => }
    }

    rpc lobby(id: Int) async throws RpcOnlyError -> Channel<Command, Event, CommandError> {
        return Channel(sending: commands()) { command => }
    }

    rpc hall(id: Int) async throws RpcOnlyError -> Channel<Command, Event, CommandError> {
        return Channel(sending: events()) { command =>
            throw OtherError.nope
        }
    }

    private func commands() -> Stream<Command> {
        return commands()
    }

    private func events() -> Stream<Event> {
        return events()
    }
}
