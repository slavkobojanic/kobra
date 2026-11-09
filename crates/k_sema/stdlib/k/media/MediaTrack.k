/// One source of a [`MediaStream`]: a microphone, a camera or a screen.
/// Its fields are reactive.
@ClientOnly
@Native
public class MediaTrack implements Equatable {
    /// `.audio` or `.video`.
    var kind: TrackKind
    /// What the browser calls the source ("MacBook Pro Microphone").
    var label: String
    /// The device it came from; `nil` for a screen.
    var device: MediaDeviceId?
    /// The app's mute: a disabled track sends silence or black frames, and
    /// keeps the device open.
    var isEnabled: Bool
    /// The source's mute: the system isn't delivering, through no action of
    /// the app.
    var isMuted: Bool
    /// `.live`, or `.ended` and why.
    var state: TrackState
    /// The size, frame rate and audio settings the browser chose.
    var settings: TrackSettings

    /// Enables or disables the track ([`isEnabled`]).
    func setEnabled(_ enabled: Bool)
    /// Ends the track and, once no track uses it, releases the device.
    func stop()
}
