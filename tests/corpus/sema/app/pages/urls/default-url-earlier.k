// A default URL conflicts with a named one: `page Pricing` is `/pricing`.
page Pricing {
    view {
        text { "pricing" }
    }
}

page Plans at "/pricing" {
    view {
        text { "plans" }
    }
}
