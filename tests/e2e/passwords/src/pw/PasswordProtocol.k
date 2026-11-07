import k.rpc.RpcOnlyError

public protocol PasswordProtocol {
    rpc roundTrip(password: String, other: String) throws RpcOnlyError -> String
    rpc unusable(password: String) throws RpcOnlyError -> Bool
    rpc against(password: String, hash: String) throws RpcOnlyError -> Bool
}
