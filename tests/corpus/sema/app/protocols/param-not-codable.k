// Rpc parameters are `Codable`.
import k.rpc.RpcOnlyError

struct Filter {
    let text: String
}

struct Query implements Codable {
    let text: String
}

protocol Search {
    rpc find(filter: Filter) throws RpcOnlyError -> Int
    rpc query(query: Query) throws RpcOnlyError -> Int
}
