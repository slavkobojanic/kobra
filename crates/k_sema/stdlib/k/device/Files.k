/// Opening files the user chooses, and saving files the app generates as
/// downloads (`docs/design/42-files.md`). A client-only platform interface:
/// `BrowserFiles` is injected with no `use` rule, and `TestFiles` replaces
/// it in tests.
///
/// `open` and `openAll` must be called from a user's action (a click
/// handler) before its first `await`: browsers show a file picker only in
/// response to one.
@Platform(side: .client)
public interface Files {
    /// Asks the user to choose one file of `types` (any file when empty)
    /// and reads it. Throws `.cancelled` when the picker is closed without
    /// a choice and `.tooLarge` for a file over `File.maximumSize`.
    func open(accepting types: List<FileType>) async throws FileOpenError -> File
    /// Asks the user to choose any number of files of `types` and reads
    /// them, in the order chosen.
    func openAll(accepting types: List<FileType>) async throws FileOpenError -> List<File>
    /// Saves `file` as a download named `file.name`.
    func save(_ file: File) throws FileSaveError
}
