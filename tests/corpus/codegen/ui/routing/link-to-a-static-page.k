// `link(to: .pricing)` to a page without parameters has a constant URL, written into the template
// (12-routing.md, "Default URLs").
page Pricing {
    view {
        text { "pricing" }
    }
}

page Home {
    view {
        link(to: .pricing) { "Pricing" }
    }
}
