// A page that needs a signed-in user.
import k.auth.Authenticated

public page Profile requires Authenticated {
    view {
        text { "profile" }
    }
}
