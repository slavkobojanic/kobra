/// The deployment's own TURN server (`docs/design/46-media-and-rtc.md`,
/// "STUN and TURN without third-party accounts"), read by [`IceServers`].
/// Server-only, like every config.
///
/// URLs without a secret are a configuration error at startup, and so is a
/// URL whose scheme isn't `stun`, `turn` or `turns`.
public config TurnConfig {
    /// The STUN and TURN URLs browsers are given, comma-separated:
    /// `turn:turn.example.org:3478?transport=udp`. Empty for none. Text,
    /// since a `Url` is an `http`, `https`, `ws` or `wss` URL; a scheme
    /// other than `stun`, `turn` or `turns` fails at startup.
    @Env("K_TURN_URLS") let urls: List<String> = []
    /// coturn's `static-auth-secret`, which signs the credentials; read
    /// from `K_TURN_SECRET_FILE` in containers.
    @Env("K_TURN_SECRET") let secret: Secret<String>? = nil
}
