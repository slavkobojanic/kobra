// Only the runtime produces `rpcFailure` (28-rpc-wire-format.md): a handler
// that throws it is K5506 at the `throw`, however the case is written. Its
// own cases are fine, and so is client code passing `rpcFailure` on.
import k.rpc.RpcFailure

enum CartError implements Error, Codable {
    case outOfStock
    case rpcFailure(RpcFailure)
}

enum LookupError implements Error {
    case missing
    case rpcFailure(RpcFailure)
}

protocol CartProtocol {
    rpc add(id: Int) throws CartError -> Int
}

service CartApi implements CartProtocol {
    rpc add(id: Int) async throws CartError -> Int {
        if id < 0 {
            throw .rpcFailure(.offline)
        }
        if id == 0 {
            throw CartError.rpcFailure(.timeout)
        }
        throw CartError.outOfStock
    }
}

service CartClient {
    @Inject let api: CartProtocol

    func add(id: Int) async throws LookupError -> Int {
        do {
            return try await api.add(id: id)
        } catch .rpcFailure(let failure) {
            throw LookupError.rpcFailure(failure)
        } catch .outOfStock {
            throw .missing
        }
    }
}
