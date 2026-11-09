// A server-rendered app reading a client-only platform interface (45-web-platform.md, "Server
// rendering and the server"): `isSupported` and `permission` may be read while the server renders,
// where they are `false` and `.prompt`, so a `@Derived` value made only of them is neither K4012 nor
// K4013 when a page reads it. The service's `@State` still is K4013.
public app Weather {
    metadata {
        title("Weather")
    }
}
