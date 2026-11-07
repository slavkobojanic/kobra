import k.rpc.RpcOnlyError

public service UpstreamApi implements UpstreamProtocol {
    @Inject let upstream: Upstream

    rpc describe(path: String, timeoutMs: Int) async throws RpcOnlyError -> String {
        return await upstream.describe(path: path, timeout: timeoutMs.milliseconds)
    }

    rpc raw(path: String) async throws RpcOnlyError -> String {
        return await upstream.raw(path: path)
    }

    rpc unreachable() async throws RpcOnlyError -> String {
        return await upstream.unreachable()
    }

    rpc create() async throws RpcOnlyError -> String {
        return await upstream.create()
    }
}
