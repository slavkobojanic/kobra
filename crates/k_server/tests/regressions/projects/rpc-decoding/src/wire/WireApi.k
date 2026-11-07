import k.crypto.Uuid
import k.rpc.RpcOnlyError
import k.text.EmailAddress

/// Echoes what it was given.
public service WireApi implements WireProtocol {
    rpc mail(to address: EmailAddress) async throws RpcOnlyError -> String {
        return "{address}"
    }

    rpc site(url: Url) async throws RpcOnlyError -> String {
        return url.host
    }

    rpc lookup(id: Uuid) async throws RpcOnlyError -> String {
        return "{id}"
    }

    rpc paint(color: Color) async throws RpcOnlyError -> String {
        return "ok"
    }

    rpc count(scores: Map<Int, String>) async throws RpcOnlyError -> Int {
        return scores.count
    }

    rpc signup(request: Signup) async throws RpcOnlyError -> String {
        return request.handle
    }

    rpc nick(request: Nick) async throws RpcOnlyError -> Int {
        return request.name.count
    }

    rpc signupAll(requests: List<Signup>) async throws RpcOnlyError -> Int {
        return requests.count
    }
}
