/// An app that turns server rendering off for every page.
@Rendering(.client)
public app ClientRendering {
    metadata {
        title("Client App")
        locale(.english)
    }
}
