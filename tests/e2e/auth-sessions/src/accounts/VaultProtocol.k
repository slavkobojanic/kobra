import k.auth.Authenticated
import k.rpc.RpcOnlyError

/// A protocol whose `requires` clause guards every rpc.
public protocol VaultProtocol requires Authenticated {
    /// A note only signed-in users may read.
    rpc note() throws RpcOnlyError -> String
}
