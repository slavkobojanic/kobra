/// The primary pointing device, [`Viewport.pointer`].
public enum PointerKind implements Equatable {
    /// A mouse or a trackpad (`pointer: fine`).
    case fine
    /// A finger (`pointer: coarse`).
    case coarse
    /// No pointing device (`pointer: none`): keyboard or voice only.
    case none
}
