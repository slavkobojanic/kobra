import k.auth.SessionManager
import k.test.TestChannels
import k.test.TestClock
import k.test.TestConnectivity
import k.test.TestPeers
import lobby.AccountId

test Presence {
    @Inject let sessions: SessionManager
    @Inject let peers: TestPeers
    @Inject let network: TestChannels
    @Inject let connectivity: TestConnectivity
    @Inject let clock: TestClock
    @Inject let halls: HallProtocol

    test "the people in a hall are listed once each, in the order they arrived" {
        await sessions.signIn(userId: AccountId(name: "ana"))
        let mine = try await halls.hall(name: "main")
        let events = mine.messages

        expect await events.next() == .snapshot(present: [Member(name: "ana", status: "here")])
        expect await events.next() == .presence(.joined(Member(name: "ana", status: "here")))

        // Two tabs of one person are one member.
        let bob = await peers.signIn(userId: AccountId(name: "bob"))
        let bobs: HallProtocol = bob.client()
        let first = try await bobs.hall(name: "main")
        let second = try await bobs.hall(name: "main")

        expect await events.next() == .presence(.joined(Member(name: "bob", status: "here")))
        expect try await halls.present(name: "main") == [Member(name: "ana", status: "here"), Member(name: "bob", status: "here")]
        expect try await halls.present(name: "elsewhere") == []

        // The member leaves with the last of its channels.
        first.close()
        expect try await halls.present(name: "main") == [Member(name: "ana", status: "here"), Member(name: "bob", status: "here")]

        second.close()
        expect await events.next() == .presence(.left(Member(name: "bob", status: "here")))
        expect try await halls.present(name: "main") == [Member(name: "ana", status: "here")]
    }

    test "a lease changes its member, and can leave before its channel closes" {
        await sessions.signIn(userId: AccountId(name: "ana"))
        let watching = try await halls.hall(name: "main")
        let events = watching.messages
        let bob = await peers.signIn(userId: AccountId(name: "bob"))
        let bobs: HallProtocol = bob.client()
        let channel = try await bobs.hall(name: "main")

        expect await events.next() == .snapshot(present: [Member(name: "ana", status: "here")])
        expect await events.next() == .presence(.joined(Member(name: "ana", status: "here")))
        expect await events.next() == .presence(.joined(Member(name: "bob", status: "here")))

        try await channel.send(.status(text: "away"))

        expect await events.next() == .presence(.updated(Member(name: "bob", status: "away")))
        expect try await halls.present(name: "main") == [Member(name: "ana", status: "here"), Member(name: "bob", status: "away")]

        try await channel.send(.leave)
        try await channel.send(.leave)

        expect await events.next() == .presence(.left(Member(name: "bob", status: "away")))
        expect channel.state == .open
    }

    test "a lease taken by an rpc ends with the call" {
        await sessions.signIn(userId: AccountId(name: "ana"))

        expect try await halls.peek(name: "main") == [Member(name: "ana (peeking)", status: "here")]
        expect try await halls.present(name: "main") == []
    }

    test "a resumed channel never left; a forgotten one leaves at once" {
        await sessions.signIn(userId: AccountId(name: "ana"))
        let bob = await peers.signIn(userId: AccountId(name: "bob"))
        let bobs: HallProtocol = bob.client()
        let watching = try await bobs.hall(name: "main")
        let events = watching.messages

        expect await events.next() == .snapshot(present: [Member(name: "bob", status: "here")])
        expect await events.next() == .presence(.joined(Member(name: "bob", status: "here")))

        let mine = try await halls.hall(name: "main")

        expect await events.next() == .presence(.joined(Member(name: "ana", status: "here")))

        // A blip in Ana's browser: her channel resumes, and she never left.
        network.drop()

        expect try await bobs.present(name: "main") == [Member(name: "bob", status: "here"), Member(name: "ana", status: "here")]

        clock.advance(by: 1.second)
        try await mine.send(.status(text: "back"))

        expect await events.next() == .presence(.updated(Member(name: "ana", status: "back")))

        // As after a deploy: the server forgets her socket, the channel's
        // request ends and its lease with it; the restart joins again.
        network.drop(resumable: false)

        expect await events.next() == .presence(.left(Member(name: "ana", status: "back")))

        clock.advance(by: 1.second)

        expect await events.next() == .presence(.joined(Member(name: "ana", status: "here")))
    }

    test "a channel that never comes back leaves when its resume window passes" {
        await sessions.signIn(userId: AccountId(name: "ana"))
        let bob = await peers.signIn(userId: AccountId(name: "bob"))
        let bobs: HallProtocol = bob.client()
        let watching = try await bobs.hall(name: "main")
        let events = watching.messages

        expect await events.next() == .snapshot(present: [Member(name: "bob", status: "here")])
        expect await events.next() == .presence(.joined(Member(name: "bob", status: "here")))

        let mine = try await halls.hall(name: "main")

        expect await events.next() == .presence(.joined(Member(name: "ana", status: "here")))

        // Ana's browser goes offline and stays away.
        connectivity.goOffline()
        clock.advance(by: 29.seconds)

        expect try await bobs.present(name: "main") == [Member(name: "bob", status: "here"), Member(name: "ana", status: "here")]

        clock.advance(by: 2.seconds)

        expect await events.next() == .presence(.left(Member(name: "ana", status: "here")))
        expect mine.state != .open
    }
}
