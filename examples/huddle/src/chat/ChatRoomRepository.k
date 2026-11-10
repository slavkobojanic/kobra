/// Queries for `ChatRoom`.
public repository ChatRoomRepository for ChatRoom {
    /// The room called `name`, or `nil`.
    func find(name: String) async -> ChatRoom? {
        return await chatRooms.first { room => room.name == name }
    }

    /// Every room, by name.
    func all() async -> List<ChatRoom> {
        return await chatRooms
            .sortedBy { room => room.name }
            .list()
    }

    /// Takes the number of `room`'s next message. Call it inside the
    /// transaction that inserts that message: the row's lock orders two
    /// posts at once.
    func nextNumber(room: String) async -> Int {
        await chatRooms
            .filter { r => r.name == room }
            .update { r => r.lastNumber += 1 }

        return (await find(name: room))?.lastNumber ?? 1
    }
}
