/// A button with an icon.
public component Button {
    @Prop let title: String

    view {
        button {
            ButtonIcon()

            title
        }
    }
}

/// A helper, visible only inside the `ui` module.
component ButtonIcon {
    view {
        text { "*" }
    }
}
