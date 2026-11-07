import k.rpc.RpcOnlyError

/// The server side of `AccountsProtocol`.
public service AccountsApi implements AccountsProtocol {
    rpc signup(request: Signup) async throws RpcOnlyError -> String {
        return request.handle
    }

    rpc signupAll(requests: List<Signup>) async throws RpcOnlyError -> Int {
        return requests.count
    }

    rpc maybe(request: Signup?) async throws RpcOnlyError -> Int {
        return 0
    }
}
