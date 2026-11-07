import k.rpc.RpcOnlyError

/// Sign-ups, one or many.
public protocol AccountsProtocol {
    /// One request.
    rpc signup(request: Signup) throws RpcOnlyError -> String

    /// Many requests: each element is validated under its index.
    rpc signupAll(requests: List<Signup>) throws RpcOnlyError -> Int

    /// An optional request.
    rpc maybe(request: Signup?) throws RpcOnlyError -> Int
}
