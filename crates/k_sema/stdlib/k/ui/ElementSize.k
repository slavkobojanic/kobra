/// The size of an element's content box, the payload of the `resized`
/// event every element has (`docs/design/45-web-platform.md`, "Element
/// events").
public struct ElementSize implements Equatable {
    /// The width in CSS pixels.
    let width: Length
    /// The height in CSS pixels.
    let height: Length
}
