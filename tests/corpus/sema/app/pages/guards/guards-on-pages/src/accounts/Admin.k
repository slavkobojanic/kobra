// A page with an authentication and a role guard.
import k.auth.Authenticated
import k.auth.Role

public page Admin requires Authenticated, Role(.admin) {
//                         ^^^^^^^^^^^^^ resolves: decl k.auth.Authenticated
    view {
        text { "admin" }
    }
}
