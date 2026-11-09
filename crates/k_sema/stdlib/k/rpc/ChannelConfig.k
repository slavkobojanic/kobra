import k.validation.Range

/// The limits of channels (`docs/design/44-channels.md`, "Limits and
/// back-pressure"), read from the environment like every stdlib config.
public config ChannelConfig {
    /// The largest encoded message, in bytes, in each direction.
    @Range(1024...16777216) @Env("K_CHANNEL_MAX_MESSAGE") let maxMessage: Int = 65536
    /// How long the server keeps a dropped channel for a resume.
    @Env("K_CHANNEL_RESUME_WINDOW") let resumeWindow: Duration = 30.seconds
    /// How often the server sends a heartbeat on every socket.
    @Env("K_CHANNEL_HEARTBEAT") let heartbeat: Duration = 25.seconds
    /// The sustained messages per second each channel may send.
    @Range(1...10000) @Env("K_CHANNEL_RATE") let rate: Int = 20
}
