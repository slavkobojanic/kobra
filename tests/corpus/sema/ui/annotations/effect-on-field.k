// `@Effect` goes on functions; on a field it is K3002. `@Prop` on a function is
// K3002 too.
component Ticker {
    @Effect let tick: Int = 0
    @State var count: Int = 0

    @Prop func reset() {
        count = 0
    }

    view {
        text { "{count}" }
    }
}
