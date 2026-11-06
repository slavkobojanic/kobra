// Multi-word page names become kebab case: `page SignIn` is `/sign-in`.
page SignIn {
    view {
        text { "sign in" }
    }
}

page Login at "/sign-in" {
    view {
        text { "login" }
    }
}
