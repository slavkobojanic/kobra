// A protocol implementation is always server-only, so injecting a client-only interface into one is
// reported as K5504 (and not also as a service on both sides).
import k.rpc.RpcOnlyError
import k.storage.LocalStorage

protocol DraftsProtocol {
    rpc count() throws RpcOnlyError -> Int
}

service DraftsApi implements DraftsProtocol {
    @Inject let storage: LocalStorage

    rpc count() async throws RpcOnlyError -> Int {
        return 0
    }
}
