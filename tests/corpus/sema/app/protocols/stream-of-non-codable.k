// A server stream's elements are `Codable`.
import k.rpc.RpcOnlyError

struct Tick {
    let at: Date
}

protocol Ticker {
    rpc ticks() throws RpcOnlyError -> Stream<Tick>
}
