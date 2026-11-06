// Six independent errors across members, view, style and metadata: each is reported exactly once and none hides the others.
component T {
    @State var count: Int = 0
    + +

    view {
        button { "Add" }
            .disabled(!ready)
            .on(click: { count += 1 })

        text { "a" } text { "b" }
    }

    style {
        .a {
            color: red
            padding: .md;
        }

        .b { margin: 0 !important; }
    }

    metadata {
        title("x");
    }
}
