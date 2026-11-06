component Nav {
    view {
        a(href: "/") { "Home" }
        link(to: .home) { "Home" }
    }
}

page Home {
    view {
        text { "Home" }
    }
}
