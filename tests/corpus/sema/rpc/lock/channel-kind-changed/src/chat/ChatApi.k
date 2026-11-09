import k.rpc.Channel
import k.rpc.RpcOnlyError

/// The chat's implementation.
public service ChatApi implements ChatProtocol {
    rpc history(id: Int) async throws RpcOnlyError -> Channel<String, String, RpcOnlyError> {
        return Channel(sending: lines()) { line => }
    }

    rpc room(id: Int) async throws RpcOnlyError -> List<String> {
        return []
    }

    private func lines() -> Stream<String> {
        return lines()
    }
}
