import k.rpc.RpcOnlyError

/// One echo rpc per message shape: a struct result is the response itself,
/// anything else is `{"value": …}`.
public protocol EchoProtocol {
    rpc record(value: Record) throws RpcOnlyError -> Record
    rpc shapes(value: List<Shape>) throws RpcOnlyError -> List<Shape>
    rpc maybe(value: Leaf?) throws RpcOnlyError -> Leaf?
    rpc counts(value: Map<String, Int>) throws RpcOnlyError -> Map<String, Int>
    rpc text(value: String) throws RpcOnlyError -> String
    rpc number(value: Int) throws RpcOnlyError -> Int
    rpc pair(left: Double, right: Mood) throws RpcOnlyError -> Mood
    rpc ping() throws RpcOnlyError
}
