/// Opening files the user chooses, and saving files the app generates as
/// downloads (`docs/design/42-files.md`). A client-only platform interface:
/// [`BrowserFiles`] is injected with no `use` rule, and
/// [`k.test.TestFiles`] replaces it in tests.
///
/// [`open(accepting:)`] and [`openAll(accepting:)`] must be called from a
/// user's action (a click handler) before its first `await`: browsers show
/// a file picker only in response to one.
@Platform(side: .client)
public interface Files {
    /// Asks the user to choose one file and reads it.
    ///
    /// - Parameter types: the kinds of file offered; any file when empty.
    /// - Returns: the chosen file, read in full.
    /// - Throws: [`FileOpenError.cancelled`] when the picker is closed
    ///   without a choice, [`FileOpenError.tooLarge`] for a file over
    ///   [`File.maximumSize`] and [`FileOpenError.unreadable`] when the
    ///   file can't be read.
    func open(accepting types: List<FileType>) async throws FileOpenError -> File
    /// Asks the user to choose any number of files and reads them.
    ///
    /// - Parameter types: the kinds of file offered; any file when empty.
    /// - Returns: the chosen files, in the order chosen.
    /// - Throws: as [`open(accepting:)`], for any of the files.
    func openAll(accepting types: List<FileType>) async throws FileOpenError -> List<File>
    /// Saves `file` as a download named after its [`File.name`].
    ///
    /// - Throws: [`FileSaveError.unavailable`] when the browser refuses
    ///   downloads.
    func save(_ file: File) throws FileSaveError
}
