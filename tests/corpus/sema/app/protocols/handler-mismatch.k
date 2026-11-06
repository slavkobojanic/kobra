// Handlers repeat their rpc's signature, `throws` included; a missing handler is a missing member.
import k.rpc.RpcFailure
import k.rpc.RpcOnlyError

enum CartError implements Error, Codable {
    case empty
    case rpcFailure(RpcFailure)
}

protocol CartProtocol {
    rpc count() throws CartError -> Int
    rpc clear() throws RpcOnlyError
}

service CartApi implements CartProtocol {
    rpc count() async throws RpcOnlyError -> Int {
        return 0
    }

    rpc clear() async throws RpcOnlyError {
    }
}

protocol TagsProtocol {
    rpc list() throws RpcOnlyError -> List<String>
}

service TagsApi implements TagsProtocol {
}
