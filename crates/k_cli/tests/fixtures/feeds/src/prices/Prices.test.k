import k.encoding.Json
import k.http.DecodingError
import k.http.WebSocketMessage
import k.test.TestClock
import k.test.TestWebSocketClient
import k.test.TestWebSocketServer
import k.time.Backoff

test Prices {
    @Inject let sockets: TestWebSocketClient
    @Inject let clock: TestClock

    test "ticks from the feed appear on the page" {
        let feed = TestWebSocketServer()
        sockets.serve(url: Url("wss://feed.example/v1"), with: feed)
        let screen = render(Prices())
        await screen.settle()

        expect feed.connections == 1
        expect feed.received == [.text("subscribe BTC")]
        expect sockets.requests.map { r => r.subprotocol } == ["feed.v2"]

        feed.send(.text(Json.encode(Tick(symbol: "BTC", cents: 4200))))
        feed.send(.text("not a tick"))
        feed.send(.binary(Json.encode(Tick(symbol: "BTC", cents: 4300)).utf8))
        await screen.settle()

        expect screen.status().text == "BTC 4300"
        expect not screen.alert().exists
    }

    test "a dropped feed reconnects after the backoff" {
        let feed = TestWebSocketServer()
        sockets.serve(url: Url("wss://feed.example/v1"), with: feed)
        let screen = render(Prices())
        await screen.settle()

        feed.close(code: 1011, reason: "restarting")
        await screen.settle()

        expect screen.alert().text == "Reconnecting…"
        expect feed.connections == 0

        clock.advance(by: 1.second)
        await screen.settle()

        expect feed.connections == 1
        expect not screen.alert().exists
        expect feed.received == [.text("subscribe BTC"), .text("subscribe BTC")]
    }

    test "leaving the page closes the socket" {
        let feed = TestWebSocketServer()
        sockets.serve(url: Url("wss://feed.example/v1"), with: feed)
        let screen = render(Prices())
        await screen.settle()

        screen.unmount()
        await screen.settle()

        expect feed.connections == 0
    }
}

test Relay {
    @Inject let sockets: TestWebSocketClient
    @Inject let relay: RelayProtocol

    test "a server service's socket uses the test's double" {
        let feed = TestWebSocketServer()
        sockets.serve(url: Url("wss://feed.example/v1"), with: feed)

        let chosen = try await relay.relay(text: "hello")

        expect chosen == "feed.v2"
        expect feed.received == [.text("hello")]
        expect sockets.requests.map { r => r.headers.get("x-feed-key") } == ["k_1"]
    }

    test "a refusal set with fail is thrown to the caller" {
        sockets.fail(url: Url("wss://feed.example/v1"), error: .refused(status: .forbidden))

        expect try await relay.relay(text: "hello") == "refused 403"
    }
}

test Backoffs {
    test "delays grow to 30 seconds and start again after reset" {
        let backoff = Backoff()
        let first = backoff.next()
        var later = first

        for _ in 0..<10 {
            later = backoff.next()
        }

        expect first < 500.milliseconds
        expect later < 30.seconds
        backoff.reset()
        expect backoff.next() < 500.milliseconds
    }

    test "a message decodes as JSON, or says why it can't" {
        let tick: Tick = try WebSocketMessage.text(Json.encode(Tick(symbol: "ETH", cents: 7))).json()

        expect tick.cents == 7
        expect throws DecodingError.malformed {
            let bad: Tick = try WebSocketMessage.text("nope").json()
        }
    }
}
