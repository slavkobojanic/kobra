/// The app of the `k test` and real-Chrome fixture for generics. It keeps
/// nothing on a server, so `k build` emits static files.
@Rendering(.client)
public app Generics {
    metadata {
        title("Generics")
    }
}
