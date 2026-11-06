// An rpc without a result still declares `throws`; the fix goes after the parameter list.
import k.rpc.RpcOnlyError

protocol Store {
    rpc ping()
    rpc pong() throws RpcOnlyError
}
