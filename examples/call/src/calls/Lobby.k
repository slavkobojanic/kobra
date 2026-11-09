import k.auth.Authenticated

/// The start page: start a call, then send its link to the other person.
public page Lobby requires Authenticated at "/" {
    @Inject let rooms: RoomStarter

    @State var problem: String? = nil

    func start() async {
        problem = nil

        do {
            let room = try await rooms.create()
            Router.navigate(to: .room(id: room))
        } catch .rpcFailure {
            problem = "Couldn't reach Call. Check your connection and try again."
        }
    }

    metadata {
        title("Call")
    }

    view {
        heading(level: 1) { "Call" }

        paragraph { "Start a call, then send its link to the person you want to talk to. They open it in their browser, and you see and hear each other." }

        button { "Start a call" }
            .on(click: start)

        if let problem {
            paragraph { problem }
                .role(.alert)
        }
    }
}
