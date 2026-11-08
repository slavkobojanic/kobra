// Only a protocol's rpcs are served (11-protocols-and-rpc.md), so an `rpc` member anywhere else is
// K5508 at its keyword: in a struct, a component or a service that implements no protocol declaring
// it. The fix makes it a `func`; serving it means declaring it in a protocol the service implements.
// A handler of a protocol's rpc may be an `rpc` or a `func`, and a struct that implements a protocol
// is K5504 instead.
import k.rpc.RpcOnlyError

protocol Notes {
    rpc list() throws RpcOnlyError -> Int
}

service NotesApi implements Notes {
    rpc list() async throws RpcOnlyError -> Int {
        return 0
    }

    rpc count() async throws RpcOnlyError -> Int {
        return 1
    }
}

service Plain {
    rpc ping() async throws RpcOnlyError {
    }
}

struct Point {
    let x: Int

    rpc norm() async throws RpcOnlyError -> Int {
        return x
    }
}

component Counter {
    rpc reset() async throws RpcOnlyError {
    }

    view {
        text { "0" }
    }
}

service FuncHandlers implements Notes {
    func list() async throws RpcOnlyError -> Int {
        return 2
    }
}
