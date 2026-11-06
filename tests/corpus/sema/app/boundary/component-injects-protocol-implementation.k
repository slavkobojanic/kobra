// A service that implements a protocol is server-only, so a component can't inject it.
import k.rpc.RpcOnlyError

protocol CountProtocol {
    rpc count() throws RpcOnlyError -> Int
}

service CountApi implements CountProtocol {
    rpc count() async throws RpcOnlyError -> Int {
        return 0
    }
}

component Counter {
    @Inject let api: CountApi

    view {
        text { "count" }
    }
}
