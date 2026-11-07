/// The standard `Files`: the browser's file picker (an `input` of type
/// `file`) and downloads through an object URL.
@ClientOnly
@Native
public service BrowserFiles implements Files {
    /// Opens the browser's file picker for one file.
    func open(accepting types: List<FileType>) async throws FileOpenError -> File
    /// Opens the browser's file picker for several files.
    func openAll(accepting types: List<FileType>) async throws FileOpenError -> List<File>
    /// Starts a download of `file`.
    func save(_ file: File) throws FileSaveError
}
