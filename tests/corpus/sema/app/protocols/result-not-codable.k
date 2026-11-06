// Rpc results are `Codable`.
import k.rpc.RpcOnlyError

struct Report {
    let lines: List<String>
}

protocol Reports {
    rpc latest() throws RpcOnlyError -> Report
    rpc count() throws RpcOnlyError -> Int
}
