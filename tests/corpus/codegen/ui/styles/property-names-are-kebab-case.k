// Camel-case K property names are the CSS names in kebab case (06-styles.md).
component Panel {
    view {
        box { "x" }
            .class(.panel)
    }

    style {
        .panel {
            borderLeft: 4px solid #000000;
            backgroundColor: #FFFFFF;
            maxHeight: 400px;
            fontWeight: bold;
        }
    }
}
