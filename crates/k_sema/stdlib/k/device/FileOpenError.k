/// Why `Files.open` or `Files.openAll` returned no file
/// (`docs/design/42-files.md`).
public enum FileOpenError implements Error, Equatable {
    /// The user closed the picker without choosing a file.
    case cancelled
    /// A chosen file is larger than `limit` bytes (`File.maximumSize`).
    case tooLarge(limit: Int)
    /// The browser couldn't read a chosen file, or can't show a picker.
    case unreadable
}
