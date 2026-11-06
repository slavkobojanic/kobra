// Protocol-level `requires` names guards, like pages.
import k.rpc.RpcOnlyError

struct Banner {
    let text: String
}

protocol Ping requires Banner(text: "x") {
    rpc ping() throws RpcOnlyError
}
