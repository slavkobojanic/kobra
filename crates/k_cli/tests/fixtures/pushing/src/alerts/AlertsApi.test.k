import k.device.PushSubscription
import k.device.SystemNotification
import k.encoding.Json
import k.test.RecordingWebPush
import k.test.SentPush

test AlertsApi {
    @Inject let push: RecordingWebPush
    @Inject let api: AlertsApi

    /// A subscription as a browser would send it.
    func subscription(_ n: Int) -> PushSubscription {
        let endpoint = Url("https://push.example/subscriptions").appending(path: "{n}")
        return PushSubscription(endpoint: endpoint, p256dh: "BCVxsr7N_eNgVRqvHtD0zTZsEc6-VV-JvLexhqUzORcxaOzi6-AYWXvTBHm4bjyPjs7Vd8pZGH6SRpkNtoIAiw4", auth: "BTBZMqHH6r4Tts7J_aSIgg")
    }

    test "a ping is recorded with its page and time to live" {
        expect try await api.ping(subscription: subscription(1), text: "Hello") == "sent"

        expect push.sent == [SentPush(message: SystemNotification(title: "Ping", body: "Hello", tag: "ping", opens: .report(id: "7")), subscription: subscription(1), timeToLive: 1.minutes)]
    }

    test "the server key is the double's, until it is unconfigured" {
        expect try await api.serverKey() != nil

        push.setConfigured(false)

        expect try await api.serverKey() == nil
        expect try await api.ping(subscription: subscription(1), text: "Hello") == "not configured"
        expect push.sent.isEmpty
    }

    test "an expired subscription says so, and the others still get it" {
        push.expire(subscription(1))

        expect try await api.ping(subscription: subscription(1), text: "Hello") == "expired"
        expect try await api.ping(subscription: subscription(2), text: "Hello") == "sent"
        expect push.sent.count == 1
    }

    test "a failure lasts until it stops" {
        push.simulateFailure(error: .rejected(status: 403))

        expect try await api.ping(subscription: subscription(1), text: "Hello") == "rejected 403"

        push.simulateFailure(error: .unavailable)

        expect try await api.ping(subscription: subscription(1), text: "Hello") == "unavailable"

        push.stopFailing()

        expect try await api.ping(subscription: subscription(1), text: "Hello") == "sent"
    }

    test "an endpoint that isn't https is refused before sending" {
        let plain = PushSubscription(endpoint: Url("http://push.example/subscriptions/1"), p256dh: "x", auth: "y")

        expect try await api.ping(subscription: plain, text: "Hello") == "invalid subscription"
        expect push.sent.isEmpty
    }

    test "a message over 4 KB is too large" {
        let long = (1...1000).map { n => "words " }.joined(separator: "")

        expect try await api.ping(subscription: subscription(1), text: long) == "too large"
    }

    test "a subscription survives JSON" {
        let back: PushSubscription = try Json.decode(Json.encode(subscription(3)))

        expect back == subscription(3)
    }
}
