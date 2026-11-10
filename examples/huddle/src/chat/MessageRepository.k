import k.crypto.Uuid

/// Queries for `Message`.
public repository MessageRepository for Message {
    /// Every message of `room`, oldest first.
    func all(room: String) async -> List<Message> {
        return await messages
            .filter { message => message.room == room }
            .sortedBy { message => message.number }
            .list()
    }

    /// How many messages of `room` after number `after` someone other than
    /// `member` posted: their unread count.
    func unread(room: String, after: Int, for member: String) async -> Int {
        return await messages
            .filter { message => message.room == room and message.number > after and message.author != member }
            .count()
    }

    /// A message of `room` by id, or `nil`.
    func find(id: Uuid, room: String) async -> Message? {
        return await messages.first { message => message.id == id and message.room == room }
    }
}
