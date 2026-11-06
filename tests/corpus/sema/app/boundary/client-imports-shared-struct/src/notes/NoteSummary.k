// A shared struct in the same module as server-only code.
public struct NoteSummary implements Codable {
    let title: String
}

entity Note {
    @Id let id: Int
    let title: String
}

repository NoteRepository for Note {}
