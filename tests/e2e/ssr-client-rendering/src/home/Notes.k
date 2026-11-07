/// Notes, loaded in the browser.
public service Notes {
    /// The note to show; its text is a marker the shell must not contain.
    func first() async -> Note {
        return Note(text: "LOADED-ON-THE-SERVER")
    }
}
