/// A kind of file the picker offers (`docs/design/42-files.md`): its media
/// type and file name extension.
public enum FileType implements Equatable {
    /// JSON, `.json`.
    case json
    /// Plain text, `.txt`.
    case text
    /// Comma-separated values, `.csv`.
    case csv
    /// Markdown, `.md`.
    case markdown
    /// A PNG image, `.png`.
    case png
    /// A JPEG image, `.jpg`.
    case jpeg
    /// An SVG image, `.svg`.
    case svg
    /// A PDF document, `.pdf`.
    case pdf
    /// Any other kind: its media type and extension, without the dot.
    case other(mediaType: String, fileExtension: String)

    /// The media type ("application/json"), as the table of
    /// `docs/design/42-files.md` gives it.
    @Native
    var mediaType: String

    /// The file name extension, without the dot ("json").
    @Native
    var fileExtension: String
}
