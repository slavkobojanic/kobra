import k.auth.SessionManager
import k.test.TestChannels
import k.test.TestClock
import k.test.TestConnectivity

test Lobby {
    @Inject let sessions: SessionManager
    @Inject let network: TestChannels
    @Inject let connectivity: TestConnectivity
    @Inject let clock: TestClock
    @Inject let api: LobbyProtocol

    test "the snapshot comes first, then what is said" {
        try await api.announce(text: "welcome")
        await sessions.signIn(userId: AccountId(name: "ana"))
        let screen = render(Lobby())
        await screen.settle()

        expect screen.item(containing: "welcome").isVisible
        expect not screen.status().exists

        screen.textField(labeled: "Message").fill("hi")
        screen.button(named: "Send").click()
        await screen.settle()

        expect screen.items().map { item => item.text } == ["welcome", "ana: hi"]
    }

    test "a refused message throws the refusal and the channel stays open" {
        await sessions.signIn(userId: AccountId(name: "ana"))
        let screen = render(Lobby())
        await screen.settle()

        screen.button(named: "Send").click()
        await screen.settle()

        expect screen.alert().text == "Say something first."

        screen.textField(labeled: "Message").fill("still here")
        screen.button(named: "Send").click()
        await screen.settle()

        expect screen.item(containing: "ana: still here").isVisible
    }

    test "the test's connection lists its open channels" {
        await sessions.signIn(userId: AccountId(name: "ana"))
        let screen = render(Lobby())
        await screen.settle()

        expect network.open.map { c => c.method } == ["/realtime.lobby.LobbyProtocol/Lobby"]

        screen.unmount()
        await screen.settle()

        expect network.open.isEmpty
    }

    test "a dropped connection shows a banner and resumes without losing a line" {
        await sessions.signIn(userId: AccountId(name: "ana"))
        let screen = render(Lobby())
        await screen.settle()

        network.drop()
        await screen.settle()

        expect screen.status().text == "Reconnecting…"

        try await api.announce(text: "said while away")
        clock.advance(by: 1.second)
        await screen.settle()

        expect not screen.status().exists
        expect screen.items().map { item => item.text } == ["said while away"]
    }

    test "a channel the server forgot restarts with a new snapshot" {
        await sessions.signIn(userId: AccountId(name: "ana"))
        let screen = render(Lobby())
        await screen.settle()

        network.drop(resumable: false)
        try await api.announce(text: "said while away")
        clock.advance(by: 1.second)
        await screen.settle()

        expect not screen.status().exists
        expect screen.items().map { item => item.text } == ["said while away"]
        expect network.open.count == 1
    }

    test "offline, the channel waits until the connection is back" {
        await sessions.signIn(userId: AccountId(name: "ana"))
        let screen = render(Lobby())
        await screen.settle()

        connectivity.goOffline()
        await screen.settle()
        clock.advance(by: 1.minute)
        await screen.settle()

        expect screen.status().text == "Reconnecting…"

        connectivity.goOnline()
        await screen.settle()

        expect not screen.status().exists
    }

    test "a quiet channel stays open for hours of virtual time" {
        await sessions.signIn(userId: AccountId(name: "ana"))
        let screen = render(Lobby())
        await screen.settle()

        clock.advance(by: 3.hours)
        await screen.settle()

        expect not screen.status().exists
        expect network.open.count == 1
    }

    test "signing out closes the channel" {
        await sessions.signIn(userId: AccountId(name: "ana"))
        let screen = render(Lobby())
        await screen.settle()

        await sessions.signOut()
        await screen.settle()

        expect network.open.isEmpty
    }
}
