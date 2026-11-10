import accounts.AccountId
import k.auth.SessionManager
import k.crypto.Uuid
import k.test.TestChannels
import k.test.TestClock
import k.test.TestConnectivity
import k.test.TestPageVisibility
import k.test.TestPeers

test Lobby {
    @Inject let sessions: SessionManager
    @Inject let peers: TestPeers
    @Inject let network: TestChannels
    @Inject let visibility: TestPageVisibility
    @Inject let clock: TestClock
    @Inject let connectivity: TestConnectivity

    /// What the lobby lists, one line per room.
    func rooms(_ screen: Screen) -> List<String> {
        return screen.list(named: "Rooms").items().map { item => item.text }
    }

    /// Ana's lobby, after she opened `#general` once (that makes her a
    /// member, so it has a count).
    func anasLobby() async -> Screen {
        await sessions.signIn(userId: AccountId(name: "ana"))
        let room = render(Room(name: "general"))
        await room.settle()
        room.unmount()

        let screen = render(Lobby())
        await screen.settle()
        return screen
    }

    test "a room's unread count rises live as others post, and clears once it is read" {
        let screen = await anasLobby()

        expect rooms(screen) == ["#general", "#help", "#random"]

        let bob = await peers.signIn(userId: AccountId(name: "bob"))
        let chat: ChatProtocol = bob.client()
        let channel = try await chat.room(name: "general")
        try await channel.send(.post(text: "one", clientId: Uuid.random()))
        try await channel.send(.post(text: "two", clientId: Uuid.random()))
        await screen.settle()

        expect rooms(screen) == ["#general 2 unread", "#help", "#random"]

        // Read in another tab of Ana's: this lobby hears it.
        let room = render(Room(name: "general"))
        await room.settle()
        await screen.settle()

        expect rooms(screen) == ["#general", "#help", "#random"]
    }

    test "a deleted message no longer counts, and one's own never do" {
        let screen = await anasLobby()
        let bob = await peers.signIn(userId: AccountId(name: "bob"))
        let chat: ChatProtocol = bob.client()
        let channel = try await chat.room(name: "general")
        let first = Uuid.random()
        try await channel.send(.post(text: "oops", clientId: first))
        try await channel.send(.post(text: "hi", clientId: Uuid.random()))

        var oops: ChatMessage? = nil
        for await event in channel.messages {
            match event {
                case .posted(let message, let clientId) {
                    if clientId == first {
                        oops = message
                        break
                    }
                }
                default { }
            }
        }

        let again = try await chat.room(name: "general")
        try await again.send(.delete(id: oops?.id ?? Uuid.random()))

        let anas: ChatProtocol = (await peers.signIn(userId: AccountId(name: "ana"))).client()
        let mine = try await anas.room(name: "general")
        try await mine.send(.post(text: "mine", clientId: Uuid.random()))
        await screen.settle()

        expect rooms(screen) == ["#general 1 unread", "#help", "#random"]
    }

    test "a room never joined has no count" {
        let screen = await anasLobby()
        let bob = await peers.signIn(userId: AccountId(name: "bob"))
        let chat: ChatProtocol = bob.client()
        let channel = try await chat.room(name: "random")
        try await channel.send(.post(text: "anyone?", clientId: Uuid.random()))
        await screen.settle()

        expect rooms(screen) == ["#general", "#help", "#random"]
    }

    test "a hidden tab's messages stay unread until it is shown" {
        let screen = await anasLobby()
        let room = render(Room(name: "general"))
        await room.settle()
        visibility.hide()
        await room.settle()

        let bob = await peers.signIn(userId: AccountId(name: "bob"))
        let chat: ChatProtocol = bob.client()
        let channel = try await chat.room(name: "general")
        try await channel.send(.post(text: "look", clientId: Uuid.random()))
        await room.settle()
        await screen.settle()

        expect rooms(screen) == ["#general 1 unread", "#help", "#random"]

        visibility.show()
        await room.settle()
        await screen.settle()

        expect rooms(screen) == ["#general", "#help", "#random"]
    }

    test "coming back to the lobby never shows the counts it had when it was left" {
        let screen = await anasLobby()
        let bob = await peers.signIn(userId: AccountId(name: "bob"))
        let chat: ChatProtocol = bob.client()
        let channel = try await chat.room(name: "general")
        try await channel.send(.post(text: "one", clientId: Uuid.random()))
        await screen.settle()

        expect rooms(screen) == ["#general 1 unread", "#help", "#random"]

        // Read while the lobby isn't shown: nothing tells the old list.
        screen.unmount()
        let room = render(Room(name: "general"))
        await room.settle()
        room.unmount()

        // Back to the lobby before its new snapshot arrives.
        connectivity.goOffline()
        let again = render(Lobby())
        await again.settle()

        expect not rooms(again).contains("#general 1 unread")

        again.unmount()
        connectivity.goOnline()
        let online = render(Lobby())
        await online.settle()

        expect rooms(online) == ["#general", "#help", "#random"]
    }

    test "after the server forgot the lobby's channel, a new snapshot has the counts" {
        let screen = await anasLobby()

        network.drop(resumable: false)
        let bob = await peers.signIn(userId: AccountId(name: "bob"))
        let chat: ChatProtocol = bob.client()
        let channel = try await chat.room(name: "general")
        try await channel.send(.post(text: "missed", clientId: Uuid.random()))
        clock.advance(by: 1.second)
        await screen.settle()

        expect rooms(screen) == ["#general 1 unread", "#help", "#random"]
    }

    test "creating a room opens it, with its creator as moderator" {
        await sessions.signIn(userId: AccountId(name: "ana"))
        let screen = render(Lobby())
        await screen.settle()

        screen.textField(labeled: "Room name").fill(" Team ")
        screen.button(named: "Create room").click()
        await screen.settle()

        // The screen shows the room it navigated to, inside the app's
        // layout, as the browser would.
        expect screen.currentPage == .room(name: "team")
        expect screen.heading(named: "#team").exists
        expect screen.button(named: "Sign out").exists
        expect screen.list(named: "In this room").items().map { item => item.text } == ["ana (moderator)"]

        let other = render(Lobby())
        await other.settle()

        expect rooms(other) == ["#general", "#help", "#random", "#team"]
    }

    test "a room's link opens it, and the layout's link comes back" {
        await sessions.signIn(userId: AccountId(name: "ana"))
        let screen = render(Lobby())
        await screen.settle()

        screen.link(named: "#help").click()
        await screen.settle()

        expect screen.currentPage == .room(name: "help")
        expect screen.heading(named: "#help").exists

        screen.link(named: "Huddle").click()
        await screen.settle()

        expect screen.currentPage == .lobby
        expect rooms(screen) == ["#general", "#help", "#random"]
    }

    test "a room's name is checked, and taken only once" {
        await sessions.signIn(userId: AccountId(name: "ana"))
        let screen = render(Lobby())
        await screen.settle()

        screen.textField(labeled: "Room name").fill("two words")
        screen.button(named: "Create room").click()
        await screen.settle()

        expect screen.alert().text == "Use 1 to 24 characters, without spaces or / ? # %."

        screen.textField(labeled: "Room name").fill("general")
        screen.button(named: "Create room").click()
        await screen.settle()

        expect screen.alert().text == "There's a room called #general already."
    }

    test "the lobby lists a room someone creates, live" {
        await sessions.signIn(userId: AccountId(name: "ana"))
        let screen = render(Lobby())
        await screen.settle()

        let bob = await peers.signIn(userId: AccountId(name: "bob"))
        let lobby: LobbyProtocol = bob.client()
        let channel = try await lobby.lobby()
        try await channel.send(.create(name: "secret", password: "hunter2"))
        await screen.settle()

        expect rooms(screen) == ["#general", "#help", "#random", "#secret (password)"]
    }
}
