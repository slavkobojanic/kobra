/// The app of the `k test` and real-Chrome fixture for indexed
/// collections. The entries live in the browser's IndexedDB, so the app
/// renders in the browser only and `k build` emits static files.
@Rendering(.client)
public app Notebook {
    metadata {
        title("Notebook")
    }
}
