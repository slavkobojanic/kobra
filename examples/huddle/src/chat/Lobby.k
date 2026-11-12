import k.auth.Authenticated

/// The rooms, each a link with its unread count, kept live; and a form to
/// create one.
public page Lobby requires Authenticated at "/" {
    @Inject let lobby: LobbyService

    @State var newName: String = ""
    @State var newPassword: String = ""
    @State var problem: String? = nil

    /// Follows the lobby while the page is shown.
    @Effect
    func follow() async {
        do {
            try await lobby.follow()
        } catch .rpcFailure(.rejected) {
            Router.navigate(to: .signIn)
        } catch .rpcFailure {
            problem = "Couldn't load the rooms. Check your connection and reload."
        }
    }

    /// Creates the room and opens it: its creator is its moderator.
    func create() async {
        problem = nil

        let name = roomName(typed: newName)

        do {
            try await lobby.create(name: name, password: newPassword)
            newName = ""
            newPassword = ""
            Router.navigate(to: .room(name: name))
        } catch .invalidName {
            problem = "Use 1 to 24 characters, without spaces or / ? # %."
        } catch .nameTaken {
            problem = "There's a room called #{name} already."
        } catch .rpcFailure {
            problem = "Couldn't create the room. Try again."
        }
    }

    metadata {
        title("Rooms · Huddle")
    }

    view {
        heading(level: 1) { "Rooms" }

        list {
            for room in lobby.rooms key room.name {
                item {
                    link(to: .room(name: room.name)) { "#{room.name}" }

                    if room.locked {
                        text { " (password)" }
                            .class(.muted)
                    }

                    if let unread = room.unread, unread > 0 {
                        text { " {unread} unread" }
                            .class(.unread)
                    }
                }
            }
        }
        .label("Rooms")

        form {
            textField(value: bind newName)
                .label("Room name")

            passwordField(value: bind newPassword)
                .label("Password (optional)")

            button { "Create room" }
                .type(.submit)
        }
        .class(.create)
        .label("New room")
        .on(submit: create)

        if let problem {
            paragraph { problem }
                .class(.problem)
                .role(.alert)
        }
    }

    style {
        .muted { color: .muted; }
        .unread { fontWeight: bold; color: .primary; }
        .create { display: flex; gap: .sm; flexWrap: wrap; }
        .problem { color: .danger; }
    }
}
