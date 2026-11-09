/// What a capture's stream belongs to: it stops with it
/// (`docs/design/46-media-and-rtc.md`, "Lifetimes").
public enum CaptureLifetime implements Equatable {
    /// The run that captured it: stopped when its component unmounts or
    /// its page is left.
    case owner
    /// The injecting service's instance: the route, for `@Scope(.route)`.
    case service
}
