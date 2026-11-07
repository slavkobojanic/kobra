/// The error enum of an rpc with no errors of its own:
/// `rpc featured() throws RpcOnlyError -> List<Product>`.
public enum RpcOnlyError implements Error, Codable {
    /// A failure outside the handler ([`RpcFailure`]).
    case rpcFailure(RpcFailure)
}
