import k.device.File

/// What a `video` or `audio` element plays
/// (`docs/design/46-media-and-rtc.md`, "Showing and playing media").
public enum MediaSource {
    /// A live stream; a local one plays no sound unless `.monitor(true)`.
    case stream(MediaStream)
    /// A file, such as a recording, through an object URL the element
    /// revokes when the source changes or it unmounts.
    case file(File)
    /// A file on the web.
    case url(Url)
}
