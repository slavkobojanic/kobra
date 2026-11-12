import accounts.AccountId
import k.auth.SessionManager
import k.crypto.Uuid
import k.test.TestPeer
import k.test.TestPeers

test Moderation {
    @Inject let sessions: SessionManager
    @Inject let peers: TestPeers

    /// Ana creates `#team` from her own tab (she moderates it), which
    /// opens it on the same screen.
    func anasTeam() async -> Screen {
        await sessions.signIn(userId: AccountId(name: "ana"))
        let screen = render(Lobby())
        await screen.settle()
        screen.textField(labeled: "Room name").fill("team")
        screen.button(named: "Create room").click()
        await screen.settle()
        return screen
    }

    /// Bob, in `#team` on a second browser.
    func bob() async -> TestPeer {
        return await peers.signIn(userId: AccountId(name: "bob"))
    }

    test "a moderator deletes anyone's message, and everyone sees it go" {
        let screen = await anasTeam()
        let chat: ChatProtocol = (await bob()).client()
        let channel = try await chat.room(name: "team")
        try await channel.send(.post(text: "spam", clientId: Uuid.random()))
        await screen.settle()

        screen.button(named: "Delete “spam”").click()
        await screen.settle()

        expect not screen.item(containing: "spam").exists

        let events = channel.messages
        expect await events.next() == .snapshot(messages: [], present: [Member(name: "ana"), Member(name: "bob")])
        expect await events.next() == .moderators(names: ["ana"])

        var deleted = false
        while not deleted {
            match await events.next() {
                case .deleted(_) { deleted = true }
                default {}
            }
        }
        expect deleted
    }

    test "someone who isn't a moderator is refused, and the channel stays open" {
        let screen = await anasTeam()
        screen.textField(labeled: "Message").fill("mine")
        screen.button(named: "Send").click()
        await screen.settle()

        let chat: ChatProtocol = (await bob()).client()
        let channel = try await chat.room(name: "team")
        var anas: ChatMessage? = nil
        for await event in channel.messages {
            match event {
                case .snapshot(let all, _) { anas = all.first }
                default {}
            }

            break
        }

        let again = try await chat.room(name: "team")

        expect throws CommandError.notYourMessage {
            try await again.send(.delete(id: anas?.id ?? Uuid.random()))
        }
        expect throws CommandError.notModerator {
            try await again.send(.promote(name: "bob"))
        }
        expect throws CommandError.notModerator {
            try await again.send(.remove(name: "ana"))
        }

        try await again.send(.post(text: "still here", clientId: Uuid.random()))
        await screen.settle()

        expect screen.item(containing: "still here").isVisible
    }

    test "a moderator makes another member a moderator" {
        let screen = await anasTeam()
        let chat: ChatProtocol = (await bob()).client()
        let channel = try await chat.room(name: "team")
        await screen.settle()

        screen.button(named: "Make bob a moderator").click()
        await screen.settle()

        expect screen.list(named: "In this room").items().map { item => item.text } == ["ana (moderator)", "bob (moderator)"]

        // Bob may now moderate: removing a moderator is refused, nobody is
        // removed by a typo.
        expect throws CommandError.cannotRemoveModerator {
            try await channel.send(.remove(name: "ana"))
        }
        expect throws CommandError.noSuchMember {
            try await channel.send(.remove(name: "nobody"))
        }
    }

    test "a removed member's channel ends with their removal, and they can't come back" {
        let screen = await anasTeam()
        let bob = await bob()
        let chat: ChatProtocol = bob.client()
        let channel = try await chat.room(name: "team")
        await screen.settle()

        screen.button(named: "Remove bob").click()
        await screen.settle()

        expect screen.list(named: "In this room").items().map { item => item.text } == ["ana (moderator)"]

        screen.textField(labeled: "Message").fill("after")
        screen.button(named: "Send").click()
        await screen.settle()

        // The server ends Bob's channel: its removal is the last message,
        // and the channel closes `byServer`.
        let events = channel.messages
        var last: ChatEvent? = nil
        while let event = await events.next() {
            last = event
        }

        expect last == .removed(name: "bob")
        expect channel.state == .closed(.byServer)
        expect throws CommandError.rpcFailure(.offline) {
            try await channel.send(.post(text: "let me in", clientId: Uuid.random()))
        }
        expect throws RoomError.removed {
            try await chat.room(name: "team")
        }
        expect not screen.item(containing: "let me in").exists
    }

    test "a removed member's page says so and closes the room" {
        // Bob created `#team`; Ana joins it, and Bob removes her.
        let bob = await bob()
        let lobby: LobbyProtocol = bob.client()
        let lobbyChannel = try await lobby.lobby()
        try await lobbyChannel.send(.create(name: "team", password: nil))

        await sessions.signIn(userId: AccountId(name: "ana"))
        let screen = render(Room(name: "team"))
        await screen.settle()

        let chat: ChatProtocol = bob.client()
        let channel = try await chat.room(name: "team")
        try await channel.send(.remove(name: "ana"))
        await screen.settle()

        expect screen.status().text == "A moderator removed you from #team."
        expect not screen.textField(labeled: "Message").exists

        let again = render(Room(name: "team"))
        await again.settle()

        expect again.alert().text == "A moderator removed you from #team."
    }
}
