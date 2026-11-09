// In an app that is already client-rendered, `@Rendering(.client)` on a component has no
// effect: a warning whose fix removes it.
@Rendering(.client)
public app Shop {
    metadata {
        title("Shop")
    }
}
