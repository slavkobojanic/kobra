/// The app of the `k test` and real-Chrome fixture for element
/// references. It keeps nothing on a server, so `k build` emits static
/// files.
@Rendering(.client)
public app Focusing {
    metadata {
        title("Focusing")
    }
}
