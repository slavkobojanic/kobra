import k.media.MediaDeviceId
import k.test.TestClock
import k.test.TestMediaDevices

test MemosPage {
    @Inject let devices: TestMediaDevices
    @Inject let clock: TestClock

    test "a blocked microphone says how to allow it" {
        devices.setPermission(.microphone, to: .denied)

        let screen = render(MemosPage())

        expect not screen.button(named: "Record").exists
        expect screen.alert().text == "Microphone access is blocked. Allow it in your browser's site settings to record."
    }

    test "a recorded memo appears in the list" {
        let screen = render(MemosPage())

        screen.button(named: "Record").click()
        await screen.settle()
        devices.microphone.speak(level: 0.5, for: 3.seconds)
        clock.advance(by: 3.seconds)
        screen.button(named: "Stop").click()
        await screen.settle()

        expect screen.item(containing: "0:03").exists
        expect devices.liveTracks.isEmpty
    }

    test "leaving the page while recording turns the microphone off" {
        let screen = render(MemosPage())

        screen.button(named: "Record").click()
        await screen.settle()
        screen.unmount()

        expect devices.liveTracks.isEmpty
    }

    test "refusing the prompt says how to allow the microphone" {
        devices.answerPrompts(with: .denied)
        let screen = render(MemosPage())

        screen.button(named: "Record").click()
        await screen.settle()

        expect devices.prompts == 1
        expect not screen.button(named: "Record").exists
        expect screen.alert().text == "Microphone access is blocked. Allow it in your browser's site settings to record."
    }

    test "paused time isn't recorded" {
        let screen = render(MemosPage())

        screen.button(named: "Record").click()
        await screen.settle()
        clock.advance(by: 2.seconds)
        screen.button(named: "Pause").click()
        clock.advance(by: 5.seconds)
        screen.button(named: "Resume").click()
        clock.advance(by: 1.seconds)
        screen.button(named: "Stop").click()
        await screen.settle()

        expect screen.item(containing: "0:03").exists
    }

    test "unplugging the microphone keeps what was recorded and says so" {
        let screen = render(MemosPage())

        screen.button(named: "Record").click()
        await screen.settle()
        clock.advance(by: 2.seconds)
        devices.remove(MediaDeviceId(value: "test-microphone"))
        await screen.settle()

        expect screen.status().text == "The microphone was disconnected, so recording stopped. The memo was saved."
        expect screen.item(containing: "0:02").exists
        expect devices.liveTracks.isEmpty
    }
}
