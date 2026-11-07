import k.device.File
import k.device.FileOpenError
import k.device.FileSaveError
import k.device.FileType
import k.device.Files

/// The default `Files` in `k test` (`docs/design/42-files.md`): no picker
/// opens. The test says what the user chooses before the code under test
/// asks, and reads what was saved.
@TestDefault(replaces: Files)
@Native
public service TestFiles implements Files {
    /// Every file saved so far, oldest first.
    var saved: List<File>
    /// The `types` of every `open` and `openAll` so far, oldest first.
    var requests: List<List<FileType>>

    /// Answers with the next chosen file, or throws `.cancelled` when the
    /// test chose none.
    func open(accepting types: List<FileType>) async throws FileOpenError -> File
    /// Answers with every chosen file, or throws `.cancelled` when the test
    /// chose none.
    func openAll(accepting types: List<FileType>) async throws FileOpenError -> List<File>
    /// Records `file` in `saved`, or throws the error set by `failSaves`.
    func save(_ file: File) throws FileSaveError

    /// The user chooses `file` at the next `open` or `openAll`.
    func choose(_ file: File)
    /// The next `open` or `openAll` throws `error`.
    func failOpen(error: FileOpenError)
    /// Makes every following `save` throw `error`.
    func failSaves(error: FileSaveError)
}
