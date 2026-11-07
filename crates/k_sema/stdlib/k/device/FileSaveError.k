/// Why [`Files.save`] couldn't start a download
/// (`docs/design/42-files.md`).
public enum FileSaveError implements Error, Equatable {
    /// The browser refuses downloads here, as some embedded views do.
    case unavailable
}
