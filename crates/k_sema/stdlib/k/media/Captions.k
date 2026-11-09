/// The captions of a `video` element: a video from a file or a URL without
/// them is a warning (WCAG 1.2.2).
public enum Captions implements Equatable {
    /// A WebVTT file.
    case track(url: Url, language: String, label: String)
    /// The video has no speech, so it needs no captions.
    case notNeeded
}
