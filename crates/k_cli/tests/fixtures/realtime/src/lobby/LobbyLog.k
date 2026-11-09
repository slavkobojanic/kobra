import k.rpc.Broadcast

/// The lobby's lines and their fan-out, one per server process.
@Scope(.server)
public service LobbyLog {
    private let events: Broadcast<String, LobbyEvent> = Broadcast()
    private var lines: List<String> = []

    /// Every line so far.
    func all() -> List<String> {
        return lines
    }

    /// Adds `line` and announces it.
    func add(_ line: String) {
        lines.append(line)
        events.publish(key: "lobby", value: LobbyEvent.said(line: line))
    }

    /// The lines added from now on.
    func subscribe() -> Stream<LobbyEvent> {
        return events.subscribe(key: "lobby")
    }
}
