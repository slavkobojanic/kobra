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

    /// `true` while the room asks for its password.
    @State var locked: Bool = false
    @State var password: String = ""

    /// One more after the password is accepted, which runs `follow` again.
    @State var unlocks: Int = 0

    /// Follows the room while the page is shown; leaving the page closes the
    /// channel.
    @Effect(on: [name, unlocks])
    func follow() async {
        problem = nil
        locked = false

        do {
            try await room.follow(room: name)
        } catch .noSuchRoom {
            problem = "There's no room called “{name}”."
        } catch .passwordRequired, .wrongPassword {
            locked = true
        } catch .removed {
            problem = "A moderator removed you from #{name}."
        } catch .rpcFailure(.rejected) {
            Router.navigate(to: .signIn.returnTo(.room(name: name)))
        } catch .rpcFailure {
            problem = "Couldn't join #{name}. Check your connection and reload."
        }
    }

    /// Gives the room's password; once it is accepted the room opens, and
    /// stays open to this person from then on.
    func unlock() async {
        do {
            try await room.unlock(room: name, password: password)
            password = ""
            unlocks += 1
        } catch .wrongPassword {
            problem = "That isn't the password of #{name}."
        } catch .rpcFailure(.rejected(.rateLimited)) {
            problem = "Too many tries. Wait a minute, then try again."
        } catch _ {
            problem = "Couldn't join #{name}. Check your connection and reload."
        }
    }

    /// Marks what's on screen read when the tab is shown again.
    @Effect(on: [room.isSeen])
    func seen() async {
        await room.markRead()
    }

    /// Tells the room this person is typing while the draft changes.
    @Effect(on: [draft])
    func announceTyping() async {
        if draft.isNotEmpty {
            await room.typed()
        }
    }

    /// Who else is typing, as a sentence, or `nil`.
    @Derived let typingLine: String? = typingSentence(names: room.typing.filter { n => n != auth.user?.id.name })

    func post() async {
        let text = draft

        draft = ""

        do {
            try await room.post(text: text)
            problem = nil
        } catch .invalidText {
            draft = text
            problem = "Write 1 to 500 characters."
        } catch _ {
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
        } catch _ {
            problem = "Couldn't delete that message. Try again."
        }
    }

    /// A moderator's action on the member `name`, with what to say when it
    /// is refused.
    func moderate(name: String, promoting: Bool) async {
        do {
            if promoting {
                try await room.promote(name: name)
            } else {
                try await room.remove(name: name)
            }
        } catch .notModerator {
            problem = "Only a moderator can do that."
        } catch .cannotRemoveModerator {
            problem = "{name} is a moderator, so they stay."
        } catch _ {
            problem = "Couldn't do that. Try again."
        }
    }

    metadata {
        title("#{name} · Huddle")
    }

    view {
        heading(level: 1) { "#{name}" }

        AlertSwitch(room: name)

        if locked {
            form {
                paragraph { "#{name} has a password. You'll be asked for it once." }

                passwordField(value: bind password)
                    .label("Room password")

                button { "Join #{name}" }
                    .type(.submit)
            }
            .class(.composer)
            .on(submit: unlock)
        }

        if room.removed {
            paragraph { "A moderator removed you from #{name}." }
                .role(.status)
        } else if let banner = room.banner {
            paragraph { banner }
                .class(.banner)
                .role(.status)
        }

        list {
            for member in room.present key member.name {
                item {
                    text { member.name }

                    if room.moderators.contains(member.name) {
                        text { " (moderator)" }
                            .class(.sending)
                    } else if room.isModerator {
                        button { "Make {member.name} a moderator" }
                            .on(click: { await moderate(name: member.name, promoting: true) })

                        button { "Remove {member.name}" }
                            .on(click: { await moderate(name: member.name, promoting: false) })
                    }
                }
            }
        }
        .label("In this room")
        .class(.present)

        list {
            for message in room.messages key message.id {
                item {
                    text { message.author }
                        .class(.author)

                    text { " {message.text}" }

                    if message.author == auth.user?.id.name or room.isModerator {
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

        if let typingLine {
            paragraph { typingLine }
                .class(.typing)
        }

        if not locked and not room.removed {
            form {
                textField(value: bind draft)
                    .label("Message")

                button { "Send" }
                    .type(.submit)
            }
            .class(.composer)
            .on(submit: post)
        }

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
        } catch _ {
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
        .present  { display: flex; gap: .sm; color: .muted; }
        .typing   { color: .muted; }
        .problem  { color: .danger; }
    }
}

/// "bob is typing…", "bob and cy are typing…", or `nil` for nobody.
func typingSentence(names: List<String>) -> String? {
    match names.count {
        case 0 { return nil }
        case 1 { return "{names.joined(separator: "")} is typing…" }
        default { return "{names.joined(separator: " and ")} are typing…" }
    }
}
