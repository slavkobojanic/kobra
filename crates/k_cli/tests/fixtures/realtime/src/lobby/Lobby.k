/// The lobby, live: its lines, a banner while the channel isn't open, and
/// a field to say something.
public page Lobby at "/" {
    @Inject let lobby: LobbyService

    @State var draft: String = ""
    @State var problem: String? = nil

    @Effect
    func follow() async {
        do {
            try await lobby.follow()
        } catch _ {
            problem = "Couldn't join the lobby."
        }
    }

    func say() async {
        do {
            try await lobby.say(text: draft)
            draft = ""
            problem = nil
        } catch .empty {
            problem = "Say something first."
        } catch .rpcFailure {
            problem = "Couldn't send."
        }
    }

    view {
        if let banner = lobby.banner {
            paragraph { banner }
                .role(.status)
        }

        list {
            for line in lobby.lines key line {
                item { line }
            }
        }

        form {
            textField(value: bind draft)
                .label("Message")

            button { "Send" }
                .type(.submit)
        }
        .on(submit: say)

        if let problem {
            paragraph { problem }
                .role(.alert)
        }
    }
}
