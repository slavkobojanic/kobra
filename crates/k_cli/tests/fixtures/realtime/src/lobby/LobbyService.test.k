import k.test.TestChannel
import k.test.TestClock

test LobbyService {
    @Inject let lobby: LobbyService
    @Inject let clock: TestClock

    test "messages delivered on a scripted channel reach the service" {
        let room: TestChannel<LobbyCommand, LobbyEvent, CommandError> = TestChannel()
        room.deliver(.snapshot(lines: ["one"]))
        room.deliver(.said(line: "two"))
        room.close(.byServer)
        await lobby.listen(on: room.channel)

        expect lobby.lines == ["one", "two"]
        expect lobby.connection == .closed(.byServer)
    }

    test "what the service sends is recorded, and a refusal is thrown" {
        let room: TestChannel<LobbyCommand, LobbyEvent, CommandError> = TestChannel()
        lobby.attach(room.channel)

        try await lobby.say(text: "hello")
        room.refuseNext(with: .empty)

        expect throws CommandError.empty {
            try await lobby.say(text: "")
        }
        expect room.sent == [.say(text: "hello")]
    }

    test "a dropped channel reconnects, and sends wait for it" {
        let room: TestChannel<LobbyCommand, LobbyEvent, CommandError> = TestChannel()
        lobby.attach(room.channel)

        room.drop()

        expect lobby.banner == "Reconnecting…"

        clock.advance(by: 1.second)

        expect lobby.banner == nil
    }

    test "a closed channel refuses to send" {
        let room: TestChannel<LobbyCommand, LobbyEvent, CommandError> = TestChannel()
        lobby.attach(room.channel)

        room.close(.byClient)

        expect lobby.banner == "Disconnected."
        expect throws CommandError.rpcFailure(.offline) {
            try await lobby.say(text: "anyone?")
        }
    }
}
