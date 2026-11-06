// Without an `AuthRole` enum in the app, `Role(…)` has no cases to name.
import k.auth.Role

page Admin requires Role(.admin) {
    view {
        text { "admin" }
    }
}
