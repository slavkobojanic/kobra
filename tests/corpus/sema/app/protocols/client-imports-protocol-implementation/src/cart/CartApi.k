// The server-only implementation.
import k.rpc.RpcOnlyError

public service CartApi implements CartProtocol {
    rpc count() async throws RpcOnlyError -> Int {
        return 0
    }
}
