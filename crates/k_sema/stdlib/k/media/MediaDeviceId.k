/// The id of a [`MediaDevice`]. `Codable`, so a preferred microphone can be
/// kept in `LocalStorage`.
public struct MediaDeviceId implements Equatable, Hashable, Codable {
    /// The browser's `deviceId`.
    let value: String
}
