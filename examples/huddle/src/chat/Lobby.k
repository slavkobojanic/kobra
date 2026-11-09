import k.auth.Authenticated

/// The rooms, each a link.
public page Lobby requires Authenticated at "/" {
    metadata {
        title("Rooms · Huddle")
    }

    view {
        heading(level: 1) { "Rooms" }

        list {
            for name in Rooms.names key name {
                item {
                    link(to: .room(name: name)) { "#{name}" }
                }
            }
        }
        .label("Rooms")
    }
}
