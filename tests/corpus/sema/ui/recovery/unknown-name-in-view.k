// An unknown name in a view is K1201 only, also when it is used as a handler, a
// class condition or a bound place.
component Counter {
    @State var count: Int = 0

    view {
        text { "{cnt}" }
        button { "Add" }
            .on(click: incremnt)
            .class(.big when isBig)
        textField(value: bind nme)
    }

    style {
        .big { fontSize: 20px; }
    }
}
