/// The sign-in page the `Authenticated` guard redirects to.
public page SignIn {
    @Param let returnTo: Page = .home

    view {
        paragraph { "sign in, then continue to {returnTo.url}" }
    }
}
