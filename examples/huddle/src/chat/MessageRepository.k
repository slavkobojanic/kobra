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

    /// The number the next message of `room` gets. Call it inside the
    /// transaction that inserts that message.
    func nextNumber(room: String) async -> Int {
        let count = await messages
            .filter { message => message.room == room }
            .count()

        return count + 1
    }

    /// A message of `room` by id, or `nil`.
    func find(id: Uuid, room: String) async -> Message? {
        return await messages.first { message => message.id == id and message.room == room }
    }
}
