import k.rpc.RpcFailure

/// Why a balance can't be read (`rpc.lock`).
public enum BalanceError {
    case unknownAccount(name: String)
    case rpcFailure(RpcFailure)
}
