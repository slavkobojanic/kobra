import accounts.AuthService
import k.auth.Authenticated
import k.ui.VisuallyHidden

/// One room, live: its messages, this tab's pending ones, a banner while the
/// connection is down, and a field to post.
public page Room requires Authenticated {
    /// The room's name, `general` in `/room/general`.
    @Param let name: String

    @Inject let room: RoomService
    @Inject let auth: AuthService

    @State var draft: String = ""
    @State var problem: String? = nil

    /// Follows the room while the page is shown; leaving the page closes the
    /// channel.
    @Effect(on: [name])
    func follow() async {
        problem = nil

        do {
            try await room.follow(room: name)
        } catch .noSuchRoom {
            problem = "There's no room called “{name}”."
        } catch .rpcFailure(.rejected) {
            Router.navigate(to: .signIn.returnTo(.room(name: name)))
        } catch .rpcFailure {
            problem = "Couldn't join #{name}. Check your connection and reload."
        }
    }

    func post() async {
        let text = draft

        draft = ""

        do {
            try await room.post(text: text)
            problem = nil
        } catch .invalidText {
            draft = text
            problem = "Write 1 to 500 characters."
        } catch .notYourMessage, .messageGone, .rpcFailure {
            // Shown on the message itself, with a retry button.
        }
    }

    func remove(message: ChatMessage) async {
        do {
            try await room.delete(message: message)
        } catch .notYourMessage {
            problem = "You can only delete your own messages."
        } catch .messageGone {
            // Already gone; its `deleted` event removes it.
        } catch .invalidText, .rpcFailure {
            problem = "Couldn't delete that message. Try again."
        }
    }

    metadata {
        title("#{name} · Huddle")
    }

    view {
        heading(level: 1) { "#{name}" }

        if let banner = room.banner {
            paragraph { banner }
                .class(.banner)
                .role(.status)
        }

        list {
            for message in room.messages key message.id {
                item {
                    text { message.author }
                        .class(.author)

                    text { " {message.text}" }

                    if message.author == auth.user?.id.name {
                        button {
                            "Delete"

                            VisuallyHidden { " “{message.text}”" }
                        }
                        .on(click: { await remove(message: message) })
                    }
                }
            }

            for message in room.pending key message.clientId {
                item {
                    text { message.text }

                    if message.failed {
                        text { " Not sent." }
                            .class(.failed)

                        button {
                            "Retry"

                            VisuallyHidden { " “{message.text}”" }
                        }
                        .on(click: { await retry(message: message) })
                    } else {
                        text { " Sending…" }
                            .class(.sending)
                    }
                }
                .class(.pending)
            }
        }
        .label("Messages")

        form {
            textField(value: bind draft)
                .label("Message")

            button { "Send" }
                .type(.submit)
        }
        .class(.composer)
        .on(submit: post)

        if let problem {
            paragraph { problem }
                .class(.problem)
                .role(.alert)
        }

        link(to: .lobby) { "All rooms" }
    }

    func retry(message: PendingMessage) async {
        do {
            try await room.retry(clientId: message.clientId)
        } catch .invalidText, .notYourMessage, .messageGone, .rpcFailure {
            // Marked failed again on the message.
        }
    }

    style {
        .banner   { padding: .sm; backgroundColor: .surfaceAlt; }
        .author   { fontWeight: bold; }
        .pending  { opacity: 0.7; }
        .failed   { color: .danger; }
        .sending  { color: .muted; }
        .composer { display: flex; gap: .sm; }
        .problem  { color: .danger; }
    }
}
