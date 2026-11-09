import k.crypto.Uuid
import k.test.TestChannel
import k.test.TestClock

test RoomService {
    @Inject let rooms: RoomService
    @Inject let clock: TestClock

    test "a post shows as pending until its echo replaces it" {
        let room: TestChannel<ChatCommand, ChatEvent, CommandError> = TestChannel()
        rooms.attach(room.channel)

        try await rooms.post(text: "Hello")

        let pending = rooms.pending.first
        expect pending?.text == "Hello"
        expect room.sent == [.post(text: "Hello", clientId: pending?.clientId ?? Uuid.random())]

        rooms.apply(event: .posted(message: message(number: 1, text: "Hello"), clientId: pending?.clientId ?? Uuid.random()))

        expect rooms.pending.isEmpty
        expect rooms.messages.map { m => m.text } == ["Hello"]
    }

    test "events apply in the room's order, and twice is the same as once" {
        let first = message(number: 1, text: "one")
        let second = message(number: 2, text: "two")

        rooms.apply(event: .snapshot(messages: [first]))
        rooms.apply(event: .posted(message: second, clientId: Uuid.random()))
        rooms.apply(event: .posted(message: second, clientId: Uuid.random()))
        rooms.apply(event: .snapshot(messages: [first, second]))

        expect rooms.messages == [first, second]

        rooms.apply(event: .deleted(id: first.id))

        expect rooms.messages == [second]
    }

    test "a refused post leaves nothing pending" {
        let room: TestChannel<ChatCommand, ChatEvent, CommandError> = TestChannel()
        rooms.attach(room.channel)
        room.refuseNext(with: .invalidText)

        expect throws CommandError.invalidText {
            try await rooms.post(text: " ")
        }
        expect rooms.pending.isEmpty
    }

    test "a post that can't be delivered stays, marked failed, and a retry sends it" {
        let room: TestChannel<ChatCommand, ChatEvent, CommandError> = TestChannel()
        rooms.attach(room.channel)
        room.close(.byServer)

        expect throws CommandError.rpcFailure(.offline) {
            try await rooms.post(text: "Later")
        }
        expect rooms.pending.map { p => p.failed } == [true]
        expect rooms.banner == "Disconnected. Reload the page to rejoin."

        let next: TestChannel<ChatCommand, ChatEvent, CommandError> = TestChannel()
        rooms.attach(next.channel)

        try await rooms.retry(clientId: rooms.pending.first?.clientId ?? Uuid.random())

        expect next.sent.count == 1
        expect rooms.pending.map { p => p.failed } == [false]
    }

    test "a dropped channel shows the banner until it is back" {
        let room: TestChannel<ChatCommand, ChatEvent, CommandError> = TestChannel()
        rooms.attach(room.channel)

        room.drop()

        expect rooms.banner == "Reconnecting…"

        clock.advance(by: 1.second)

        expect rooms.banner == nil
    }

    /// A stored message of `ana`'s, numbered `number`.
    func message(number: Int, text: String) -> ChatMessage {
        return ChatMessage(id: Uuid.random(), number: number, author: "ana", text: text, sentAt: clock.now)
    }
}
