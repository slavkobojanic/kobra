// K4013 in a component offers to render only that component in the browser first (inserting
// `loading { }` when it has none), then the whole app. A component also used outside a
// client-rendered subtree still warns.
public app Shop {
    metadata {
        title("Shop")
    }
}
