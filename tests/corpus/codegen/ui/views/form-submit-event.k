// `form { … }.on(submit: save)` listens to `submit` on the form element (05-views.md).
component Newsletter {
    @State var sent: Bool = false

    func save() {
        sent = true
    }

    view {
        form {
            button { "Subscribe" }
                .type(.submit)
        }
        .on(submit: save)
    }
}
