// `@Effect(on: [userId])` subscribes only to the listed values: the body runs untracked, so its
// own reads (`name`) don't re-run it (03-reactivity.md, "Escape hatches").
import k.observe.Log

component Profile {
    @Prop let userId: Int
    @State var name: String = ""

    @Effect(on: [userId])
    func report() {
        Log.info("{userId} {name}")
    }

    view {
        text { name }
    }
}
