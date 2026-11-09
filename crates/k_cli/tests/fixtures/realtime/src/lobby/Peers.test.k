import k.auth.SessionManager
import k.rpc.RpcOnlyError
import k.test.TestPeers

test Peers {
    @Inject let sessions: SessionManager
    @Inject let peers: TestPeers

    test "a line said by one person appears for another" {
        await sessions.signIn(userId: AccountId(name: "ana"))
        let screen = render(Lobby())
        await screen.settle()

        let bob = await peers.signIn(userId: AccountId(name: "bob"))
        let lobby: LobbyProtocol = bob.client()
        let channel = try await lobby.lobby()
        try await channel.send(.say(text: "lunch?"))
        await screen.settle()

        expect screen.item(containing: "bob: lunch?").isVisible

        screen.textField(labeled: "Message").fill("yes")
        screen.button(named: "Send").click()
        await screen.settle()

        expect screen.items().map { item => item.text } == ["bob: lunch?", "ana: yes"]
    }

    test "a peer's refusal reaches the peer" {
        await sessions.signIn(userId: AccountId(name: "ana"))
        let bob = await peers.signIn(userId: AccountId(name: "bob"))
        let lobby: LobbyProtocol = bob.client()
        let channel = try await lobby.lobby()

        expect throws CommandError.empty {
            try await channel.send(.say(text: ""))
        }
    }

    test "a guest peer is refused where the test's browser is let in" {
        await sessions.signIn(userId: AccountId(name: "ana"))
        let guest = peers.guest()
        let lobby: LobbyProtocol = guest.client()

        expect throws RpcOnlyError.rpcFailure(.rejected(.unauthenticated)) {
            try await lobby.lobby()
        }
    }

    test "ending a person's sessions elsewhere closes their channels" {
        await sessions.signIn(userId: AccountId(name: "ana"))
        let screen = render(Lobby())
        await screen.settle()

        let bob = await peers.signIn(userId: AccountId(name: "bob"))
        let lobby: LobbyProtocol = bob.client()
        let channel = try await lobby.lobby()
        await sessions.endAll(userId: AccountId(name: "bob"))
        await screen.settle()

        expect channel.state == .closed(.sessionEnded)
        expect not screen.status().exists
    }

    test "a peer signing out in its own browser restarts its channels, which are refused" {
        let bob = await peers.signIn(userId: AccountId(name: "bob"))
        let lobby: LobbyProtocol = bob.client()
        let channel = try await lobby.lobby()

        await bob.signOut()
        let screen = render(Lobby())
        await screen.settle()

        expect channel.state == .closed(.refused)
    }
}
