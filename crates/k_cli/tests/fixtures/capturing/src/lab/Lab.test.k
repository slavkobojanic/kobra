import k.test.TestClock
import k.test.TestMediaDevices

test LabPanel {
    @Inject let devices: TestMediaDevices
    @Inject let clock: TestClock

    test "capturing before asking is notAsked" {
        let screen = render(LabPanel())

        screen.button(named: "Capture").click()
        await screen.settle()

        expect screen.status().text == "capture failed: notAsked"
        expect devices.prompts == 0
    }

    test "asking prompts once, then capturing works" {
        let screen = render(LabPanel())

        screen.button(named: "Ask").click()
        await screen.settle()
        screen.button(named: "Capture").click()
        await screen.settle()

        expect devices.prompts == 1
        expect screen.status().text == "captured 1 audio"
        expect devices.liveTracks.isEmpty
    }

    test "a denied permission is denied without a prompt" {
        devices.setPermission(.microphone, to: .denied)
        let screen = render(LabPanel())

        screen.button(named: "Ask").click()
        await screen.settle()
        screen.button(named: "Capture").click()
        await screen.settle()

        expect devices.prompts == 0
        expect screen.status().text == "capture failed: denied"
    }

    test "a chosen microphone that isn't there is notFound" {
        devices.setPermission(.microphone, to: .granted)
        let screen = render(LabPanel())

        screen.button(named: "Capture missing").click()
        await screen.settle()

        expect screen.status().text == "capture failed: notFound"
    }

    test "each named format records" {
        devices.setPermission(.microphone, to: .granted)
        devices.setPermission(.camera, to: .granted)
        let screen = render(LabPanel())

        screen.button(named: "Record video").click()
        await screen.settle()
        clock.advance(by: 2.seconds)
        await screen.settle()

        expect screen.status().text == "recorded video/webm recording.webm"
    }

    test "a browser that can't capture says so" {
        devices.setSupported(false)
        let screen = render(LabPanel())

        screen.button(named: "Capture").click()
        await screen.settle()

        expect screen.text("unsupported").exists
        expect screen.status().text == "capture failed: unsupported"
    }
}
