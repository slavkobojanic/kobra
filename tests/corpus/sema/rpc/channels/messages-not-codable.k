// The messages of a channel cross the wire, so they are `Codable`; a `Secret` is not.
import k.rpc.Channel
import k.rpc.RpcOnlyError

struct Draft {
    let text: String
}

struct Token implements Codable {
    let value: Secret<String>
}

enum Event implements Codable {
    case posted(text: String)
}

protocol Chat {
    rpc room(id: Int) throws RpcOnlyError -> Channel<Draft, Event, RpcOnlyError>
    rpc keys(id: Int) throws RpcOnlyError -> Channel<Event, Secret<String>, RpcOnlyError>
    rpc tokens(id: Int) throws RpcOnlyError -> Channel<Event, Token, RpcOnlyError>
}
