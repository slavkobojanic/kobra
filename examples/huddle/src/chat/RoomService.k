import k.crypto.Uuid
import k.rpc.Channel
import k.rpc.ChannelState
import k.time.Clock
import k.time.TimerHandle

/// One room, followed live from the browser.
///
/// `follow(room:)` opens the room's channel and applies its events until the
/// caller is cancelled (the page is left). After a short drop the runtime
/// resumes the channel without losing an event; after a long one it opens it
/// again, and the new snapshot brings `messages` up to date. Either way the
/// page only shows `banner`.
public service RoomService {
    @Inject let chat: ChatProtocol
    @Inject let clock: Clock

    /// The room's messages, oldest first.
    @State var messages: List<ChatMessage> = []

    /// What this tab sent that hasn't come back yet, oldest first.
    @State var pending: List<PendingMessage> = []

    /// Who is in the room, in the order they arrived: this tab's person
    /// included, once however many tabs they have open.
    @State var present: List<Member> = []

    /// Who is typing, in the order they started.
    @State var typing: List<String> = []

    /// When each name in `typing` stops showing.
    private var typingTimers: Map<String, TimerHandle> = Map()

    /// `true` for a few seconds after this tab said it is typing, so a
    /// burst of keystrokes is one message.
    private var typingSent: Bool = false

    @State var channel: Channel<ChatCommand, ChatEvent, CommandError>? = nil

    @Derived let connection: ChannelState = channel?.state ?? .connecting

    /// What the banner says, or `nil` while the room is live.
    @Derived let banner: String? = match connection {
        case .connecting { "Connecting…" }
        case .open { nil }
        case .reconnecting { "Reconnecting…" }
        case .closed { "Disconnected. Reload the page to rejoin." }
    }

    /// Follows `room` until the caller is cancelled or the server closes the
    /// channel.
    ///
    /// - Throws: `RoomError.noSuchRoom`, and `.rpcFailure` when the first
    ///   open fails (signed out, offline). Later drops don't throw: the
    ///   runtime reconnects.
    func follow(room: String) async throws RoomError {
        let opened = try await chat.room(name: room)

        await listen(on: opened)
    }

    /// Applies the events of an open channel until it closes.
    func listen(on opened: Channel<ChatCommand, ChatEvent, CommandError>) async {
        attach(opened)

        for await event in opened.messages {
            apply(event: event)
        }
    }

    /// Sends on `opened` from now on, without reading it: `RoomService.test.k`
    /// hands it a `TestChannel`'s channel and plays the events itself.
    func attach(_ opened: Channel<ChatCommand, ChatEvent, CommandError>) {
        channel = opened
    }

    /// Posts `text`: it shows as pending at once, and is replaced by the
    /// stored message when the room echoes it.
    ///
    /// - Throws: `CommandError.invalidText`; `.rpcFailure(.offline)` when it
    ///   couldn't be delivered, and then it stays, marked failed, for
    ///   `retry(clientId:)`.
    func post(text: String) async throws CommandError {
        let message = PendingMessage(clientId: Uuid.random(), text: text)

        pending.append(message)

        try await deliver(message: message)
    }

    /// Sends a failed message again.
    func retry(clientId: Uuid) async throws CommandError {
        guard let message = pending.firstWhere({ p => p.clientId == clientId }) else {
            return
        }

        let again = PendingMessage(clientId: clientId, text: message.text)

        replace(clientId: clientId, with: again)

        try await deliver(message: again)
    }

    /// Tells the room this person is typing, at most once every 3 seconds.
    /// Best-effort: nothing is shown if it doesn't arrive.
    func typed() async {
        guard let channel, not typingSent else {
            return
        }

        typingSent = true
        clock.after(3.seconds) {
            typingSent = false
        }

        do {
            try await channel.send(.typing)
        } catch .invalidText, .notYourMessage, .messageGone, .rpcFailure {
            // Typing is a hint, never worth an error.
        }
    }

    /// Deletes one of this person's messages.
    ///
    /// - Throws: `CommandError.notYourMessage`, `.messageGone`, `.rpcFailure`.
    func delete(message: ChatMessage) async throws CommandError {
        guard let channel else {
            throw CommandError.rpcFailure(.offline)
        }

        try await channel.send(.delete(id: message.id))
    }

    /// Applies one event to `messages` and `pending`. Idempotent, so an echo
    /// and a snapshot that both carry a message agree.
    func apply(event: ChatEvent) {
        match event {
            case .snapshot(let all, let people) {
                messages = all
                present = people
            }

            case .posted(let message, let clientId) {
                let others = messages.filter { m => m.id != message.id }

                messages = (others + [message]).sortedBy { m => m.number }
                pending = pending.filter { p => p.clientId != clientId }
                stopTyping(name: message.author)
            }

            case .deleted(let id) {
                messages = messages.filter { m => m.id != id }
            }

            case .presence(.joined(let member)) {
                present = present.filter { p => p.name != member.name } + [member]
            }

            case .presence(.updated(let member)) {
                present = present.map { p => if p.name == member.name { member } else { p } }
            }

            case .presence(.left(let member)) {
                present = present.filter { p => p.name != member.name }
                stopTyping(name: member.name)
            }

            case .typing(let name) {
                // Shown for 5 seconds after the last one, on the injected
                // clock, so a test advances it.
                typingTimers.get(name)?.cancel()

                if not typing.contains(name) {
                    typing.append(name)
                }

                typingTimers.set(name, clock.after(5.seconds) {
                    stopTyping(name: name)
                })
            }
        }
    }

    private func stopTyping(name: String) {
        typingTimers.get(name)?.cancel()
        typingTimers.remove(name)
        typing = typing.filter { n => n != name }
    }

    private func deliver(message: PendingMessage) async throws CommandError {
        guard let channel else {
            replace(clientId: message.clientId, with: PendingMessage(clientId: message.clientId, text: message.text, failed: true))

            throw CommandError.rpcFailure(.offline)
        }

        do {
            try await channel.send(.post(text: message.text, clientId: message.clientId))
        } catch .rpcFailure(let failure) {
            // Undelivered, or delivered and not acknowledged before a
            // restart: kept, so the person decides whether to send it again.
            replace(clientId: message.clientId, with: PendingMessage(clientId: message.clientId, text: message.text, failed: true))

            throw CommandError.rpcFailure(failure)
        } catch .invalidText {
            pending = pending.filter { p => p.clientId != message.clientId }

            throw CommandError.invalidText
        } catch .notYourMessage, .messageGone {
            pending = pending.filter { p => p.clientId != message.clientId }
        }
    }

    private func replace(clientId: Uuid, with message: PendingMessage) {
        pending = pending.map { p => if p.clientId == clientId { message } else { p } }
    }
}
