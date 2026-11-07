// Multi-word page names become kebab-case URLs (12-routing.md).
page SignIn {
    view {
        text { "sign in" }
    }
}

page ProductDetail {
    @Param let id: Int

    view {
        link(to: .signIn) { "Sign in" }
    }
}
