import site.SiteLayout

/// Server rendering: every page goes through `SiteLayout`, and the app's
/// metadata is the base every page and layout adds to.
@Layout(SiteLayout)
public app Ssr {
    metadata {
        manifest(name: "SSR Test", shortName: "SSR")
        locale(.english)
    }
}
