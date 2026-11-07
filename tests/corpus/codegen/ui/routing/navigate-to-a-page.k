// `Router.navigate(to: .home)` navigates on the client through the runtime router
// (12-routing.md, runtime.md §10).
import k.ui.Router

page Home {
    view {
        text { "home" }
    }
}

page SignOut {
    func done() {
        Router.navigate(to: .home)
    }

    view {
        button { "Done" }
            .on(click: done)
    }
}
