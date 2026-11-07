/// A file the user chose, or one the app made to save
/// (`docs/design/42-files.md`). A value: its whole content is in `bytes`.
public struct File implements Equatable {
    /// The file name, without a path ("board.json").
    let name: String
    /// The media type ("application/json"); empty when the browser doesn't
    /// know it.
    let mediaType: String
    /// The content.
    let bytes: Bytes

    /// The largest file `Files.open` reads, in bytes: 16 MiB, 16777216.
    @Native
    static var maximumSize: Int

    /// The content as UTF-8 text; `nil` if it isn't valid UTF-8.
    @Native
    var text: String?

    /// The number of bytes.
    @Native
    var size: Int

    /// A file of `text` encoded as UTF-8, with `type`'s media type.
    @Native
    static func fromText(name: String, type: FileType, text: String) -> File
}
