/// Why a capture failed (`docs/design/46-media-and-rtc.md`, "Errors").
public enum CaptureError implements Error, Equatable {
    /// No capture here: an insecure origin, a browser without it, or a
    /// permissions policy.
    case unsupported
    /// The permission is `.prompt`: call `requestAccess(for:)` from a
    /// user's action first.
    case notAsked
    /// The user or the system refused.
    case denied
    /// The screen picker was closed without a choice.
    case cancelled
    /// No device of the kind, or the chosen one is gone.
    case notFound
    /// Another application holds the device.
    case busy
    /// The device can't do what was asked; `setting` names the constraint.
    case unsatisfiable(setting: String)
}
