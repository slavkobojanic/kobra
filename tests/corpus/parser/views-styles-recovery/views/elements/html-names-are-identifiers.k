// HTML tag names (div, p, a, img, span, ul, li) parse as ordinary names; rejecting them is semantic, not a parse error.
component T {
    view {
        div {
            p { "x" }

            a(href: "/") { "home" }

            img(src: "logo.png")

            span {}

            ul {
                li { "one" }
            }
        }
    }
}
