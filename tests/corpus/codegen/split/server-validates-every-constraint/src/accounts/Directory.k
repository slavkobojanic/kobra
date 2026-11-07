/// Knows which contacts are registered.
public service Directory {
    /// `true` when nobody registered `contact` yet.
    func isFree(contact: String) async -> Bool {
        return contact != "taken@example.com"
    }
}
