/// A file the user chose, or one the app made to save
/// (`docs/design/42-files.md`). A value: its whole content is in `bytes`.
public struct File implements Equatable {
    /// The file name, without a path ("board.json").
    let name: String
    /// The media type ("application/json"); empty when the browser doesn't
    /// know it.
    let mediaType: String
    /// The whole content.
    let bytes: Bytes

    /// The largest file [`Files.open`] reads, in bytes: 16 MiB, 16777216.
    @Native
    static var maximumSize: Int

    /// The content as UTF-8 text; `nil` if it isn't valid UTF-8.
    @Native
    var text: String?

    /// The size of the content, in bytes.
    @Native
    var size: Int

    /// Makes a file of `text` encoded as UTF-8, to save with
    /// [`Files.save(_:)`].
    ///
    /// - Parameters:
    ///   - name: the file name, without a path ("board.json").
    ///   - type: the kind of file, which gives its [`mediaType`].
    ///   - text: the content.
    @Native
    static func fromText(name: String, type: FileType, text: String) -> File
}
