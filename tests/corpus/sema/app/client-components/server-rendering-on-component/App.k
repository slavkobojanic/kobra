// `@Rendering(.server)` on a component is an error: the server is the default, and a component
// can't opt back in inside a client-rendered parent.
public app Shop {
    metadata {
        title("Shop")
    }
}
