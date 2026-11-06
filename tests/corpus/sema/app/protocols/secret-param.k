// `Secret<T>` is not `Codable`, so it can't be an rpc parameter.
import k.rpc.RpcOnlyError

protocol Accounts {
    rpc rotate(key: Secret<String>) throws RpcOnlyError
}
