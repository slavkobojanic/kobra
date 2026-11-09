import k.http.WebSocketClient
import k.http.WebSocketRequest
import k.rpc.RpcOnlyError

/// The server side of `RelayProtocol`: handshake headers are server-only.
public service RelayApi implements RelayProtocol {
    @Inject let sockets: WebSocketClient

    rpc relay(text: String) async throws RpcOnlyError -> String {
        do {
            let socket = try await sockets.connect(request: WebSocketRequest(url: Url("wss://feed.example/v1")).subprotocol("feed.v2").headers(["x-feed-key": "k_1"]))
            try await socket.send(.text(text))
            socket.close(code: 1000, reason: "done")

            return socket.subprotocol ?? "none"
        } catch .refused(let status) {
            return "refused {status.code}"
        } catch _ {
            return "failed"
        }
    }
}
