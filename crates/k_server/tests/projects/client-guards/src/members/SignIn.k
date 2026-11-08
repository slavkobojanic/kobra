/// Where a guest is sent; signing in happens through the rpc.
public page SignIn {
    /// The page to continue to after signing in.
    @Param let returnTo: Page = .home

    view {
        heading(level: 1) { "Sign in" }
        link(to: .home) { "Home" }
    }
}
