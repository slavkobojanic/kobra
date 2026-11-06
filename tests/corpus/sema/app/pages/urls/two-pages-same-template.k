// Two pages with the same named URL: the later one is reported at its template.
page Pricing at "/pricing" {
    view {
        text { "pricing" }
    }
}

page Plans at "/pricing" {
    view {
        text { "plans" }
    }
}
