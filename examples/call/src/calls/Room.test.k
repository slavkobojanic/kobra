import accounts.AccountId
import k.auth.SessionManager
import k.crypto.Uuid
import k.test.TestClock
import k.test.TestMediaDevices
import k.test.TestPeerConnections
import k.test.TestPeers

/// The call page against a second person who joins through the app's own
/// protocol (`docs/design/46-media-and-rtc.md`, "Loopback peers"): the
/// guard, the handler and the `SignallingRelay` run as in production, and
/// only the media engine is in-process.
test Room {
    @Inject let rtc: TestPeerConnections
    @Inject let peers: TestPeers
    @Inject let devices: TestMediaDevices
    @Inject let sessions: SessionManager
    @Inject let clock: TestClock

    test "two people see each other and chat" {
        await sessions.signIn(userId: AccountId(name: "ana"))
        let screen = render(Room(id: callRoom))
        screen.button(named: "Join call").click()
        await screen.settle()

        expect screen.status().text == "Waiting for someone to join. Send them this page's link."
        expect screen.video(labeled: "Your camera").isPlaying

        let sam = await peers.signIn(userId: AccountId(name: "sam"))
        let samsApi: CallProtocol = sam.client()
        let other = try await rtc.peer(joining: try await samsApi.join(room: callRoom))
        other.send(devices.camera.newStream())
        try await other.channel(introductions).send(Introduction(name: "Sam"))
        await screen.settle()

        expect not screen.status().exists
        expect screen.video(labeled: "Sam's camera").isPlaying
        expect other.received(introductions) == [Introduction(name: "ana")]

        screen.textField(labeled: "Message").fill("hello")
        screen.button(named: "Send").click()
        await screen.settle()

        expect other.received(chatLines).map { line => line.text } == ["hello"]

        try await other.channel(chatLines).send(ChatLine(from: "Sam", text: "hi Ana"))
        await screen.settle()

        expect screen.list(named: "Chat").items().map { item => item.text } == ["ana: hello", "Sam: hi Ana"]
    }

    test "muting disables the microphone the other person receives" {
        await sessions.signIn(userId: AccountId(name: "ana"))
        let screen = render(Room(id: callRoom))
        screen.button(named: "Join call").click()
        await screen.settle()

        let sam = await peers.signIn(userId: AccountId(name: "sam"))
        let samsApi: CallProtocol = sam.client()
        let other = try await rtc.peer(joining: try await samsApi.join(room: callRoom))
        await screen.settle()

        expect other.remote?.audioTracks.all { t => t.isEnabled } == true

        screen.button(named: "Mute").click()
        await screen.settle()

        expect other.remote?.audioTracks.all { t => not t.isEnabled } == true
        expect screen.button(named: "Unmute").exists
    }

    test "a lost network shows reconnecting, then recovers" {
        await sessions.signIn(userId: AccountId(name: "ana"))
        let screen = render(Room(id: callRoom))
        screen.button(named: "Join call").click()
        await screen.settle()

        let sam = await peers.signIn(userId: AccountId(name: "sam"))
        let samsApi: CallProtocol = sam.client()
        let other = try await rtc.peer(joining: try await samsApi.join(room: callRoom))
        other.send(devices.camera.newStream())
        await screen.settle()

        rtc.simulate(.networkLost)
        await screen.settle()
        expect screen.status().text == "Reconnecting…"

        rtc.simulate(.networkRestored)
        await screen.settle()
        expect not screen.status().exists
    }

    test "a network that doesn't come back ends the call after 15 seconds" {
        await sessions.signIn(userId: AccountId(name: "ana"))
        let screen = render(Room(id: callRoom))
        screen.button(named: "Join call").click()
        await screen.settle()

        let sam = await peers.signIn(userId: AccountId(name: "sam"))
        let samsApi: CallProtocol = sam.client()
        let other = try await rtc.peer(joining: try await samsApi.join(room: callRoom))
        await screen.settle()

        rtc.simulate(.networkLost)
        await screen.settle()
        clock.advance(by: 15.seconds)
        await screen.settle()

        expect screen.status().text == "The connection was lost."
        expect other.state == .closed(.failed)
    }

    test "when the other person hangs up, the page says so" {
        await sessions.signIn(userId: AccountId(name: "ana"))
        let screen = render(Room(id: callRoom))
        screen.button(named: "Join call").click()
        await screen.settle()

        let sam = await peers.signIn(userId: AccountId(name: "sam"))
        let samsApi: CallProtocol = sam.client()
        let other = try await rtc.peer(joining: try await samsApi.join(room: callRoom))
        other.send(devices.camera.newStream())
        await screen.settle()

        other.close()
        await screen.settle()

        expect screen.status().text == "The other person hung up."
        expect not screen.video(labeled: "The other person's camera").exists
    }

    test "sharing the screen replaces the camera, and the browser's Stop sharing goes back" {
        await sessions.signIn(userId: AccountId(name: "ana"))
        let screen = render(Room(id: callRoom))
        screen.button(named: "Join call").click()
        await screen.settle()

        let sam = await peers.signIn(userId: AccountId(name: "sam"))
        let samsApi: CallProtocol = sam.client()
        let other = try await rtc.peer(joining: try await samsApi.join(room: callRoom))
        await screen.settle()

        expect other.remote?.videoTracks.map { t => t.label } == ["Test Camera"]

        screen.button(named: "Share screen").click()
        await screen.settle()

        expect screen.video(labeled: "Your screen").isPlaying
        expect other.remote?.videoTracks.map { t => t.label } == ["Test Screen"]
        expect other.remote?.audioTracks.count == 1

        devices.camera.stopSharing()
        await screen.settle()

        expect screen.button(named: "Share screen").exists
        expect other.remote?.videoTracks.map { t => t.label } == ["Test Camera"]
    }

    test "a third person finds the room full" {
        await sessions.signIn(userId: AccountId(name: "ana"))
        let screen = render(Room(id: callRoom))
        screen.button(named: "Join call").click()
        await screen.settle()

        let sam = await peers.signIn(userId: AccountId(name: "sam"))
        let samsApi: CallProtocol = sam.client()
        let other = try await rtc.peer(joining: try await samsApi.join(room: callRoom))
        await screen.settle()

        let cy = await peers.signIn(userId: AccountId(name: "cy"))
        let cysApi: CallProtocol = cy.client()

        expect throws CallError.roomFull {
            try await cysApi.join(room: callRoom)
        }
        expect other.state == .connected
    }

    test "blocked devices explain how to allow them instead of a Join button" {
        devices.setPermission(.camera, to: .denied)
        await sessions.signIn(userId: AccountId(name: "ana"))
        let screen = render(Room(id: callRoom))
        await screen.settle()

        expect not screen.button(named: "Join call").exists
        expect screen.alert().text == "Camera or microphone access is blocked. Allow both in your browser's site settings to call."
    }

    test "the connection's statistics show a direct path" {
        await sessions.signIn(userId: AccountId(name: "ana"))
        let screen = render(Room(id: callRoom))
        screen.button(named: "Join call").click()
        await screen.settle()

        let sam = await peers.signIn(userId: AccountId(name: "sam"))
        let samsApi: CallProtocol = sam.client()
        let other = try await rtc.peer(joining: try await samsApi.join(room: callRoom))
        await screen.settle()
        clock.advance(by: 2.seconds)
        await screen.settle()

        expect screen.list(named: "Connection").text.contains("Path: direct")
        expect other.state == .connected
    }

    test "no ICE servers are issued without a TURN server" {
        let sam = await peers.signIn(userId: AccountId(name: "sam"))
        let samsApi: CallProtocol = sam.client()

        expect try await samsApi.iceServers() == []
    }
}

/// The room every test calls in.
let callRoom: Uuid = Uuid("6f1c2a2e-0000-4000-8000-0000000000c1")
