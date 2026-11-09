// The raw WebSocket client and `Backoff`: a reconnect loop on the injected clock, typed messages.
import k.http.WebSocketClient
import k.http.WebSocketRequest
import k.http.WebSocketState
import k.time.Backoff
import k.time.Clock

struct Tick implements Codable {
    let price: Int
}

service Prices {
    @Inject let sockets: WebSocketClient
    @Inject let clock: Clock

    @State var last: Int = 0
    @State var open: Bool = false

    func follow() async {
        let backoff = Backoff()
//          ^^^^^^^ type: k.time.Backoff
        while true {
            do {
                let socket = try await sockets.connect(request: WebSocketRequest(url: Url("wss://feed.example/v1")))
                backoff.reset()
                open = socket.state == .open
                try await socket.send(.text("subscribe"))
                for await message in socket.messages {
//                        ^^^^^^^ type: k.http.WebSocketMessage
                    let tick: Tick = try message.json()
                    last = tick.price
                }
                socket.close(code: 1000, reason: "done")
            } catch .refused(let status) {
                last = status.code
            } catch _ {
            }
            let wait = backoff.next()
//              ^^^^ type: Duration
            await clock.sleep(wait)
        }
    }

    func closed(state: WebSocketState) -> Bool {
        return match state {
            case .closed(let code, _) { code != 1000 }
            case .open, .closing { false }
        }
    }
}
