// `Router.navigate(to:)` takes a `Page` value.
import k.ui.Router

page Home {
    view {
        text { "home" }
    }
}

page SignOut {
    func done() {
        Router.navigate(to: .home)
        //                  ^^^^^ type: Page
    }

    view {
        button { "Done" }
            .on(click: done)
    }
}
