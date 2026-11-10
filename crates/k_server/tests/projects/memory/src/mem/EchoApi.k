import k.rpc.RpcOnlyError

/// The server side of `EchoProtocol`.
public service EchoApi implements EchoProtocol {
    rpc echo(text: String, times: Int) async throws RpcOnlyError -> List<String> {
        var out: List<String> = []
        for n in 1...times {
            out = out + ["{text} {n}"]
        }
        return out
    }
}
