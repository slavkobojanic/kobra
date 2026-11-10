/// A room's name as typed in the lobby, the way the server stores it.
func roomName(typed: String) -> String {
    return typed.trimmed().lowercased()
}

/// `true` for 1 to 24 characters, none of them a space or `/ ? # %`, so a
/// name is one part of `/room/{name}`.
func isValidRoomName(_ name: String) -> Bool {
    return name.isNotEmpty and name.count <= 24 and not [" ", "/", "?", "#", "%"].any { c => name.contains(c) }
}
