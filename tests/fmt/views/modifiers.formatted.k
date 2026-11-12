component Notice {
    @Prop let message: String

    view {
        box { text { message } }
            .role(.status)
            .class(.notice)

        button { "Dismiss" }.type(.button)

        button { "Close" }
            .type(.button)

        box {
            text { message }
        }
        .role(.status)
        .class(.notice)

        Icons.check().class(.icon)

        paragraph { "x" }
            .class(.muted)
            .on(click: {
                dismiss()
            })
    }
}
