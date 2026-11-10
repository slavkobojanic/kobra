import k.crypto.Uuid
import k.device.PushSubscription
import k.device.SystemNotification
import k.server.JobError
import k.test.RecordingWebPush

// The job is run directly, as the worker would. Pushes go to the default
// RecordingWebPush, so nothing leaves the test.

test RoomAlertJob {
    @Inject let job: RoomAlertJob
    @Inject let push: RecordingWebPush
    @Inject let alerts: RoomAlertRepository
    @Inject let presence: RoomPresence

    /// Stores an alert of `room` for `member`'s browser number `n`.
    func follow(_ member: String, room: String = "general", browser n: Int) async -> RoomAlert {
        let alert = RoomAlert(id: Uuid.random(), room: room, member: member, endpoint: Url("https://push.example/subscriptions").appending(path: "{n}"), p256dh: "BCVxsr7N_eNgVRqvHtD0zTZsEc6-VV-JvLexhqUzORcxaOzi6-AYWXvTBHm4bjyPjs7Vd8pZGH6SRpkNtoIAiw4", auth: "BTBZMqHH6r4Tts7J_aSIgg")
        await alerts.insert(alert)
        return alert
    }

    /// Where pushes went, oldest first.
    func reached() -> List<PushSubscription> {
        return push.sent.map { s => s.subscription }
    }

    test "the room's followers are told, but not the author" {
        let bob = await follow("bob", browser: 1)
        await follow("ana", browser: 2)
        await follow("cy", room: "random", browser: 3)

        try await job.run(payload: NewMessageAlert(room: "general", author: "ana", text: "Lunch?"))

        expect reached() == [alerts.subscription(of: bob)]
        expect push.sent[0].message == SystemNotification(title: "ana in #general", body: "Lunch?", tag: "room-general", opens: .room(name: "general"))
        expect push.sent[0].timeToLive == 1.hours
    }

    test "nobody in the room right now is told" {
        await follow("bob", browser: 1)
        let cy = await follow("cy", browser: 2)
        let lease = presence.join(room: "general", member: Member(name: "bob"))

        try await job.run(payload: NewMessageAlert(room: "general", author: "ana", text: "Lunch?"))

        expect reached() == [alerts.subscription(of: cy)]
        lease.leave()
    }

    test "an expired subscription is deleted, and the others are still told" {
        let gone = await follow("bob", browser: 1)
        let cy = await follow("cy", browser: 2)
        push.expire(alerts.subscription(of: gone))

        try await job.run(payload: NewMessageAlert(room: "general", author: "ana", text: "Lunch?"))

        expect reached() == [alerts.subscription(of: cy)]
        expect (await alerts.of(room: "general")).map { a => a.member } == ["cy"]
    }

    test "an unavailable push service is retried later" {
        await follow("bob", browser: 1)
        push.simulateFailure(error: .unavailable)

        expect throws JobError.retryable {
            try await job.run(payload: NewMessageAlert(room: "general", author: "ana", text: "Lunch?"))
        }
        expect (await alerts.of(room: "general")).count == 1
    }

    test "a server without a push key fails the job for good" {
        await follow("bob", browser: 1)
        push.setConfigured(false)

        expect throws JobError.permanent {
            try await job.run(payload: NewMessageAlert(room: "general", author: "ana", text: "Lunch?"))
        }
    }
}
