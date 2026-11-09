/// The lobby, live: its notes, a status line and a message field.
public page Room at "/" {
    @Inject let rooms: RoomService

    @State var draft: String = ""
    @State var problem: String? = nil

    @Effect
    func follow() async {
        do {
            try await rooms.follow(room: "lobby")
        } catch _ {
            problem = "Couldn't join the lobby."
        }
    }

    func post() async {
        do {
            try await rooms.post(text: draft)
            draft = ""
        } catch .notAllowed {
            problem = "Not allowed."
        } catch .rpcFailure {
            problem = "Couldn't send."
        }
    }

    view {
        paragraph { rooms.status }
            .role(.status)

        list {
            for line in rooms.lines key line {
                item { line }
            }
        }

        form {
            textField(value: bind draft)
                .label("Message")

            button { "Send" }
                .type(.submit)
        }
        .on(submit: post)

        if let problem {
            paragraph { problem }
                .role(.alert)
        }
    }
}
