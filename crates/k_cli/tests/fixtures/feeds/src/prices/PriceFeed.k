import k.http.WebSocketClient
import k.http.WebSocketMessage
import k.http.WebSocketRequest
import k.time.Backoff
import k.time.Clock

/// Follows the feed in the browser, reconnecting with the channels' delays.
public service PriceFeed {
    @Inject let sockets: WebSocketClient
    @Inject let clock: Clock

    /// The last price, in cents.
    @State var cents: Int? = nil

    /// Whether the socket is open.
    @State var isLive: Bool = false

    /// How many times the feed was connected.
    @State var connects: Int = 0

    /// Follows the feed until the caller is cancelled.
    func follow() async {
        let backoff = Backoff()

        while true {
            do {
                let socket = try await sockets.connect(request: WebSocketRequest(url: Url("wss://feed.example/v1?symbol=BTC")).subprotocol("feed.v2"))
                backoff.reset()
                connects += 1
                isLive = true
                try await socket.send(.text("subscribe BTC"))

                for await message in socket.messages {
                    apply(message: message)
                }
            } catch _ {
            }

            isLive = false
            await clock.sleep(backoff.next())
        }
    }

    /// Applies one message: a tick, or anything else, which is skipped.
    func apply(message: WebSocketMessage) {
        do {
            let tick: Tick = try message.json()
            cents = tick.cents
        } catch _ {
        }
    }
}
