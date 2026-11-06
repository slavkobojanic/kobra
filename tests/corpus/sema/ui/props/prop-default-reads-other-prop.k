// A field's initializer may read a prop: `@State var count: Int = start`.
component Counter {
    @Prop let start: Int = 0
    @State var count: Int = start
    //         ^^^^^ type: Int
    //                      ^^^^^ resolves: member corpus.Counter.start

    view {
        text { "{count}" }
    }
}
