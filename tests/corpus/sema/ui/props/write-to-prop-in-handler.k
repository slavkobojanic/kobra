// A write to a prop inside an event-handler lambda in the view is K3014 as well.
component Title {
    @Prop let title: String

    view {
        button { title }
            .on(click: { title = "" })
    }
}
