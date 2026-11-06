// HTML names are compile errors with the K name as a fix: `a` → `link`. The
// arguments of the unknown element are not checked, so `href:` is not reported.
component Nav {
    view {
        a(href: "/") { "Home" }
    }
}
