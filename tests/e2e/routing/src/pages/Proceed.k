/// A `Page`-typed parameter: only a path of this app's view pages parses.
public page Proceed {
    @Param let returnTo: Page = .landing

    view {
        paragraph { "continue to {returnTo.url}" }
    }
}
