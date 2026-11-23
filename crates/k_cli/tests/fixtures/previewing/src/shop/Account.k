import k.auth.Authenticated

/// A page for signed-in users.
public page Account requires Authenticated at "/account" {
    view {
        text { "Your account" }
    }
}

// The setup signs nobody in, so the router would redirect: K7520.
preview "signed out" {
    Account()
}
