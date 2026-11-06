// An rpc error enum's `rpcFailure(RpcFailure)` case: a nested pattern for one
// failure leaves the others to a later clause; `catch .rpcFailure` alone
// matches every payload.
import k.rpc.RpcFailure

enum NoteError implements Error {
    case notFound
    case rpcFailure(RpcFailure)
}

func save() async throws NoteError {}

func run() async -> String {
    do {
        try await save()
    } catch .rpcFailure(.offline) {
        return "offline"
    } catch .rpcFailure {
        return "failed"
    } catch .notFound {
        return "gone"
    }
    return "saved"
}

func runPartial() async -> String {
    do {
        try await save()
    } catch .rpcFailure(.offline), .rpcFailure(.timeout) {
        return "offline"
    } catch .notFound {
        return "gone"
    }
    return "saved"
}
