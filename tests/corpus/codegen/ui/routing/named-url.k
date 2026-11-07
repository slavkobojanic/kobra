// A page named with `at` uses that URL, not the derived one (12-routing.md, "Named URLs").
page Pricing at "/plans" {
    view {
        text { "plans" }
    }
}

page Home {
    view {
        link(to: .pricing) { "Plans" }
    }
}
