// `requires` comes after the complete signature. Written before `throws`, it is a syntax error, and the
// rpc is not also reported as lacking `throws`.
import k.auth.RateLimit
import k.rpc.RpcOnlyError

protocol Store {
    rpc add() requires RateLimit(1.perMinute) throws RpcOnlyError -> Int
    rpc count() throws RpcOnlyError -> Int requires RateLimit(1.perMinute)
}
