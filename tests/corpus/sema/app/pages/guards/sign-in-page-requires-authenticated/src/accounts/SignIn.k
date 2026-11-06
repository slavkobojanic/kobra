// The sign-in page can't itself require a signed-in user.
import k.auth.Authenticated

public page SignIn requires Authenticated {
    view {
        text { "sign in" }
    }
}
