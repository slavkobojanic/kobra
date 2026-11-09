import k.device.Viewport
import k.device.ViewportSize

/// Decisions CSS can't make: a sheet on a phone-sized viewport, and "Tap"
/// rather than "Click" for a finger.
public component Layout {
    @Inject let viewport: Viewport

    view {
        if viewport.size == ViewportSize(width: 390px, height: 844px) {
            text { "Phone sheet" }
        } else if viewport.size == nil {
            text { "Unknown size" }
        } else {
            text { "Dialog" }
        }
        if viewport.pointer == .coarse {
            text { "Tap to open" }
        } else {
            text { "Click to open" }
        }
        if viewport.canHover == false {
            text { "No hover" }
        }
    }
}
