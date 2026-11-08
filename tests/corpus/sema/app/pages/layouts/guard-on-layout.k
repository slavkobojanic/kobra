// The server checks the guards of the page it answers, never of its layout, so a `requires` clause
// on a layout is K5014, whatever it names. A page's own clause is fine.
import k.auth.RateLimit

layout Shop requires RateLimit(10.perMinute) {
    @Slot let content: View

    view {
        content
    }
}

layout Plain requires Missing {
    @Slot let content: View

    view {
        content
    }
}

@Layout(Shop)
page Home requires RateLimit(10.perMinute) {
    view {
        text { "home" }
    }
}
