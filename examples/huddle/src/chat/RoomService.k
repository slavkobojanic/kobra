import k.crypto.Uuid
import k.rpc.Channel
import k.rpc.ChannelState

/// One room, followed live from the browser.
///
/// `follow(room:)` opens the room's channel and applies its events until the
/// caller is cancelled (the page is left). After a short drop the runtime
/// resumes the channel without losing an event; after a long one it opens it
/// again, and the new snapshot brings `messages` up to date. Either way the
/// page only shows `banner`.
public service RoomService {
    @Inject let chat: ChatProtocol

    /// The room's messages, oldest first.
    @State var messages: List<ChatMessage> = []

    /// What this tab sent that hasn't come back yet, oldest first.
    @State var pending: List<PendingMessage> = []

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
            case .snapshot(let all) {
                messages = all
            }

            case .posted(let message, let clientId) {
                let others = messages.filter { m => m.id != message.id }

                messages = (others + [message]).sortedBy { m => m.number }
                pending = pending.filter { p => p.clientId != clientId }
            }

            case .deleted(let id) {
                messages = messages.filter { m => m.id != id }
            }
        }
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
