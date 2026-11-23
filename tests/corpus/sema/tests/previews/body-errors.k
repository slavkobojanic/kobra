// The body ends with the view it renders.
component Card {
    view {
        "card"
    }
}

preview "a number" {
    1 + 2
}

preview "a statement" {
    let card = 1
}

preview "nothing" {
}

preview "setup then the view" {
    let title = "Hello"
    Card()
}
