// Protocols are implemented by services only, which are server-only.
import k.rpc.RpcOnlyError

protocol Ping {
    rpc ping() throws RpcOnlyError
}

struct LocalPing implements Ping {
    rpc ping() async throws RpcOnlyError {
    }
}
