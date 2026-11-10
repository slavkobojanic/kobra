import k.device.SystemNotification
import k.server.Job
import k.server.JobError
import k.server.WebPush

/// Pushes a new message to the room's followers (`docs/design/50-push.md`):
/// everyone who asked to be notified, except its author and whoever is in
/// the room right now, since they see it already.
///
/// `ChatApi` enqueues it in the transaction that stores the message. A
/// subscription the push service says is gone is deleted; an outage is
/// retried with backoff, which may push the message again to followers it
/// already reached (at least once, as every job). One notification per room
/// stays on screen: a later message's replaces it (`tag`).
@Retry(attempts: 5, backoff: 30.seconds)
@JobName("room-alert")
service RoomAlertJob implements Job<NewMessageAlert> {
    @Inject let alerts: RoomAlertRepository
    @Inject let presence: RoomPresence
    @Inject let push: WebPush

    func run(payload: NewMessageAlert) async throws JobError {
        let present = (await presence.members(room: payload.room)).map { m => m.name }
        let message = SystemNotification(title: "{payload.author} in #{payload.room}", body: payload.text, tag: "room-{payload.room}", opens: .room(name: payload.room))
        var unavailable = false

        for alert in await alerts.of(room: payload.room) {
            if alert.member == payload.author or present.contains(alert.member) {
                continue
            }

            do {
                try await push.send(message, to: alerts.subscription(of: alert), timeToLive: 1.hours)
            } catch .expired, .invalidSubscription {
                // The browser unsubscribed, or the subscription was never good.
                await alerts.delete(alert)
            } catch .unavailable {
                unavailable = true
            } catch .notConfigured {
                throw JobError.permanent(reason: "K_PUSH_PRIVATE_KEY isn't set")
            } catch .rejected(let status) {
                throw JobError.permanent(reason: "The push service refused the message with {status}")
            } catch .tooLarge {
                throw JobError.permanent(reason: "The message is too large to push")
            }
        }

        if unavailable {
            throw JobError.retryable(reason: "A push service is unavailable")
        }
    }
}
