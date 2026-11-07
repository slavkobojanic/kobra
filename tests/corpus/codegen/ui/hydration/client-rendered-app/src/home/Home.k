public page Home {
    @State var count: Int = 0

    view {
        button { "{count}" }
            .on(click: { count += 1 })
    }
}
