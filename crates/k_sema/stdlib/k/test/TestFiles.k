import k.device.File
import k.device.FileOpenError
import k.device.FileSaveError
import k.device.FileType
import k.device.Files

/// The default [`Files`] in `k test` (`docs/design/42-files.md`): no picker
/// opens. The test says what the user chooses before the code under test
/// asks, and reads what was saved.
@TestDefault(replaces: Files)
@Native
public service TestFiles implements Files {
    /// Every file saved so far, oldest first.
    var saved: List<File>
    /// The `types` of every [`open(accepting:)`] and
    /// [`openAll(accepting:)`] so far, oldest first.
    var requests: List<List<FileType>>

    /// Answers with the next chosen file ([`choose(_:)`]), or throws
    /// [`FileOpenError.cancelled`] when the test chose none.
    func open(accepting types: List<FileType>) async throws FileOpenError -> File
    /// Answers with every chosen file, or throws
    /// [`FileOpenError.cancelled`] when the test chose none.
    func openAll(accepting types: List<FileType>) async throws FileOpenError -> List<File>
    /// Records `file` in [`saved`], or throws the error set by
    /// [`failSaves(error:)`].
    func save(_ file: File) throws FileSaveError

    /// Makes the user choose `file` at the next [`open(accepting:)`] or
    /// [`openAll(accepting:)`].
    func choose(_ file: File)
    /// Makes the next [`open(accepting:)`] or [`openAll(accepting:)`]
    /// throw `error`.
    func failOpen(error: FileOpenError)
    /// Makes every following [`save(_:)`] throw `error`.
    func failSaves(error: FileSaveError)
}
