// Errors cross the wire too, so their payloads are `Codable`.
import k.rpc.RpcFailure

struct Trace {
    let frames: List<String>
}

enum ReportError implements Error {
    case crashed(Trace)
    case rpcFailure(RpcFailure)
}

protocol Reports {
    rpc latest() throws ReportError -> Int
}
