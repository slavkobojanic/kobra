import k.test.RecordingWebPush
import k.test.TestPushSubscriptions

test PushAlerts {
    @Inject let push: TestPushSubscriptions
    @Inject let server: RecordingWebPush
    @Inject let alerts: PushAlerts

    test "the double starts capable, at prompt, with no subscription" {
        expect push.isSupported
        expect push.permission == .prompt
        expect push.prompts == 0
        expect push.subscription == nil
        expect await push.current() == nil
        expect alerts.canOffer
    }

    test "turning on asks once, then subscribes with the server's key" {
        expect await alerts.turnOn() == "on"
        expect push.prompts == 1
        expect push.permission == .granted
        expect push.serverKey == server.serverKey
        expect alerts.subscription == push.subscription
        expect await push.current() == push.subscription

        expect await alerts.turnOn() == "on"
        expect push.prompts == 1
    }

    test "subscribing again with the same key keeps the subscription" {
        push.setPermission(.granted)
        expect await alerts.turnOn() == "on"
        let first = push.subscription

        expect await alerts.turnOn() == "on"
        expect push.subscription == first
    }

    test "turning off ends the subscription, and the next one is new" {
        push.setPermission(.granted)
        expect await alerts.turnOn() == "on"
        let first = push.subscription

        await alerts.turnOff()
        expect await push.current() == nil

        expect await alerts.turnOn() == "on"
        expect push.subscription != first
    }

    test "a refused prompt subscribes nothing and hides the offer" {
        push.answerPrompts(with: .denied)

        expect await alerts.turnOn() == "denied"
        expect push.subscription == nil
        expect not alerts.canOffer
    }

    test "a dismissed prompt is not asked" {
        push.answerPrompts(with: .prompt)

        expect await alerts.turnOn() == "not asked"
        expect push.prompts == 1
    }

    test "without a server key nothing is subscribed" {
        server.setConfigured(false)
        push.setPermission(.granted)

        expect await alerts.turnOn() == "not configured"
        expect push.subscription == nil
    }

    test "an unreachable push service is unavailable, once" {
        push.setPermission(.granted)
        push.failNextSubscribe(error: .unavailable)

        expect await alerts.turnOn() == "unavailable"
        expect await alerts.turnOn() == "on"
    }

    test "an unsupported browser offers nothing and never asks" {
        push.setSupported(false)

        expect not alerts.canOffer
        expect await alerts.turnOn() == "unsupported"
        expect push.prompts == 0
    }
}
