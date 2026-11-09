import k.device.PointerKind
import k.device.Viewport
import k.device.ViewportSize

/// The default [`Viewport`] in `k test`
/// (`docs/design/45-web-platform.md`, "Viewport"): a 1280×800 viewport
/// with a fine pointer that can hover, until the test changes one. Each
/// member is reactive, like the browser's.
@TestDefault(replaces: Viewport)
@Native
public service TestViewport implements Viewport {
    /// 1280×800 until [`resize(width:height:)`].
    var size: ViewportSize?
    /// `.fine` until [`set(pointer:)`].
    var pointer: PointerKind?
    /// `true` until [`set(canHover:)`].
    var canHover: Bool?

    /// Resizes the viewport, as turning a phone or resizing the window
    /// does.
    func resize(width: Length, height: Length)
    /// Sets the primary pointer.
    func set(pointer value: PointerKind)
    /// Sets whether the primary pointer can hover.
    func set(canHover value: Bool)
}
