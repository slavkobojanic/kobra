// `@Effect(on: [a, b])` names its dependencies; they are reactive places.
component Profile {
    @Prop let userId: Int
    @State var page: Int = 0
    @State var loads: Int = 0

    @Effect(on: [userId, page])
    //           ^^^^^^ resolves: member corpus.Profile.userId
    func reload() {
        loads += 1
    }

    @Effect(on: [])
    func start() {
        loads = 0
    }

    view {
        text { "{loads}" }
    }
}
