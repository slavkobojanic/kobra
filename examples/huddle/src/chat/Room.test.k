import accounts.AccountId
import k.auth.SessionManager
import k.crypto.Uuid
import k.test.TestChannels
import k.test.TestClock
import k.test.TestConnectivity
import k.test.TestPeers

test Room {
    @Inject let sessions: SessionManager
    @Inject let peers: TestPeers
    @Inject let network: TestChannels
    @Inject let connectivity: TestConnectivity
    @Inject let clock: TestClock

    test "a message posted by one person appears for another" {
        await sessions.signIn(userId: AccountId(name: "ana"))
        let screen = render(Room(name: "general"))
        await screen.settle()

        // Bob is a second browser with his own session, on the same server.
        let bob = await peers.signIn(userId: AccountId(name: "bob"))
        let chat: ChatProtocol = bob.client()
        let channel = try await chat.room(name: "general")
        try await channel.send(.post(text: "Lunch?", clientId: Uuid("6f1c2a2e-0000-4000-8000-000000000001")))
        await screen.settle()

        expect screen.item(containing: "Lunch?").text == "bob Lunch?"

        screen.textField(labeled: "Message").fill("Yes!")
        screen.button(named: "Send").click()
        await screen.settle()

        expect screen.list(named: "Messages").items().map { item => item.text } == ["bob Lunch?", "ana Yes!Delete “Yes!”"]
    }

    test "only a message's author can delete it" {
        await sessions.signIn(userId: AccountId(name: "ana"))
        let screen = render(Room(name: "general"))
        await screen.settle()
        screen.textField(labeled: "Message").fill("Mine")
        screen.button(named: "Send").click()
        await screen.settle()

        let bob = await peers.signIn(userId: AccountId(name: "bob"))
        let chat: ChatProtocol = bob.client()
        let channel = try await chat.room(name: "general")
        var anas: ChatMessage? = nil

        // The first event is the snapshot; leaving the loop closes that channel.
        for await event in channel.messages {
            match event {
                case .snapshot(let all, _) { anas = all.first }
                default { }
            }

            break
        }

        let again = try await chat.room(name: "general")

        expect throws CommandError.notYourMessage {
            try await again.send(.delete(id: anas?.id ?? Uuid("6f1c2a2e-0000-4000-8000-000000000002")))
        }

        screen.button(named: "Delete “Mine”").click()
        await screen.settle()

        expect not screen.item(containing: "Mine").exists
    }

    test "a message posted after a deletion gets a number of its own" {
        await sessions.signIn(userId: AccountId(name: "ana"))
        let screen = render(Room(name: "general"))
        await screen.settle()

        for text in ["one", "two"] {
            screen.textField(labeled: "Message").fill(text)
            screen.button(named: "Send").click()
            await screen.settle()
        }
        screen.button(named: "Delete “one”").click()
        await screen.settle()
        screen.textField(labeled: "Message").fill("three")
        screen.button(named: "Send").click()
        await screen.settle()

        let bob = await peers.signIn(userId: AccountId(name: "bob"))
        let chat: ChatProtocol = bob.client()
        let channel = try await chat.room(name: "general")

        match await channel.messages.next() {
            case .snapshot(let all, _) { expect all.map { m => m.number } == [2, 3] }
            default { expect false }
        }
    }

    test "a dropped connection shows a banner, then catches up without losing a message" {
        await sessions.signIn(userId: AccountId(name: "ana"))
        let screen = render(Room(name: "general"))
        await screen.settle()

        network.drop()
        await screen.settle()

        expect screen.status().text == "Reconnecting…"

        let bob = await peers.signIn(userId: AccountId(name: "bob"))
        let chat: ChatProtocol = bob.client()
        let channel = try await chat.room(name: "general")
        try await channel.send(.post(text: "Said while you were away", clientId: Uuid("6f1c2a2e-0000-4000-8000-000000000003")))

        clock.advance(by: 1.second)
        await screen.settle()

        expect not screen.status().exists
        expect screen.list(named: "Messages").items().map { item => item.text } == ["bob Said while you were away"]
    }

    test "after the server forgot the channel, a new snapshot brings the room up to date" {
        await sessions.signIn(userId: AccountId(name: "ana"))
        let screen = render(Room(name: "general"))
        await screen.settle()

        network.drop(resumable: false)
        let bob = await peers.signIn(userId: AccountId(name: "bob"))
        let chat: ChatProtocol = bob.client()
        let channel = try await chat.room(name: "general")
        try await channel.send(.post(text: "Missed it?", clientId: Uuid("6f1c2a2e-0000-4000-8000-000000000004")))
        clock.advance(by: 1.second)
        await screen.settle()

        expect not screen.status().exists
        expect screen.item(containing: "Missed it?").isVisible
    }

    test "a message sent offline is marked, and retried once back online" {
        await sessions.signIn(userId: AccountId(name: "ana"))
        let screen = render(Room(name: "general"))
        await screen.settle()

        connectivity.goOffline()
        screen.textField(labeled: "Message").fill("Anyone?")
        screen.button(named: "Send").click()
        await screen.settle()

        expect screen.item(containing: "Anyone?").text == "Anyone? Sending…"

        // A message waits at most 10 seconds for the connection.
        clock.advance(by: 11.seconds)
        await screen.settle()

        expect screen.item(containing: "Anyone?").text == "Anyone? Not sent.Retry “Anyone?”"

        connectivity.goOnline()
        await screen.settle()
        screen.button(named: "Retry “Anyone?”").click()
        await screen.settle()

        expect screen.list(named: "Messages").items().map { item => item.text } == ["ana Anyone?Delete “Anyone?”"]
    }

    test "the people in the room are listed, and leave when their channel closes" {
        await sessions.signIn(userId: AccountId(name: "ana"))
        let screen = render(Room(name: "general"))
        await screen.settle()

        expect screen.list(named: "In this room").items().map { item => item.text } == ["ana"]

        // Bob, in two tabs: listed once.
        let bob = await peers.signIn(userId: AccountId(name: "bob"))
        let chat: ChatProtocol = bob.client()
        let tab = try await chat.room(name: "general")
        let other = try await chat.room(name: "general")
        await screen.settle()

        expect screen.list(named: "In this room").items().map { item => item.text } == ["ana", "bob"]

        tab.close()
        await screen.settle()

        expect screen.list(named: "In this room").items().map { item => item.text } == ["ana", "bob"]

        other.close()
        await screen.settle()

        expect screen.list(named: "In this room").items().map { item => item.text } == ["ana"]
    }

    test "someone typing shows for a few seconds after their last keystroke" {
        await sessions.signIn(userId: AccountId(name: "ana"))
        let screen = render(Room(name: "general"))
        await screen.settle()

        let bob = await peers.signIn(userId: AccountId(name: "bob"))
        let chat: ChatProtocol = bob.client()
        let channel = try await chat.room(name: "general")
        try await channel.send(.typing)
        await screen.settle()

        expect screen.text(containing: "typing…").text == "bob is typing…"

        clock.advance(by: 4.seconds)
        try await channel.send(.typing)
        await screen.settle()
        clock.advance(by: 4.seconds)
        await screen.settle()

        expect screen.text(containing: "typing…").text == "bob is typing…"

        clock.advance(by: 2.seconds)
        await screen.settle()

        expect not screen.text(containing: "typing…").exists

        // Ana's own typing reaches Bob, but isn't shown to her.
        screen.textField(labeled: "Message").fill("Hi")
        await screen.settle()

        expect not screen.text(containing: "typing…").exists

        let events = channel.messages

        expect await events.next() == .snapshot(messages: [], present: [Member(name: "ana"), Member(name: "bob")])
        expect await events.next() == .presence(.joined(Member(name: "bob")))
        expect await events.next() == .typing(name: "bob")
        expect await events.next() == .typing(name: "bob")
        expect await events.next() == .typing(name: "ana")
    }

    test "a room that doesn't exist says so" {
        await sessions.signIn(userId: AccountId(name: "ana"))
        let screen = render(Room(name: "nowhere"))
        await screen.settle()

        expect screen.alert().text == "There's no room called “nowhere”."
    }

    test "the room's channel closes when the page is left" {
        await sessions.signIn(userId: AccountId(name: "ana"))
        let screen = render(Room(name: "general"))
        await screen.settle()

        expect network.open.map { c => c.method } == ["/huddle.chat.ChatProtocol/Room"]

        screen.unmount()
        await screen.settle()

        expect network.open.isEmpty
    }

    test "a guest can't open a room" {
        let guest = peers.guest()
        let chat: ChatProtocol = guest.client()

        expect throws RoomError.rpcFailure(.rejected(.unauthenticated)) {
            try await chat.room(name: "general")
        }
    }
}
