// A `Result` crosses the wire only when its value and its error are both
// `Codable`: an `Error` isn't, so a box never reaches a codec.
import k.rpc.RpcOnlyError

enum Bad implements Error, Codable {
    case tooBig(limit: Int)
}

enum Local implements Error {
    case lost
}

protocol Probe {
    rpc fine() throws RpcOnlyError -> Result<Int, Bad>
    rpc boxed() throws RpcOnlyError -> Result<Int, Error>
    rpc local() throws RpcOnlyError -> List<Result<Int, Local>>
    rpc given(result: Result<Int, Error>) throws RpcOnlyError -> Int
}
