// `session.user` is a `SessionUser` (not optional) in a page that `requires Authenticated`, and its
// `id` has the type of the app's one `AuthUserId`. Elsewhere it stays optional.
import k.auth.Authenticated
import k.auth.AuthUserId
import k.auth.Session
import k.crypto.Uuid

struct AccountId implements AuthUserId {
    let value: Uuid
}

page Account requires Authenticated {
    @Inject let session: Session

    view {
        text { "{session.user.id.value}" }
        //       ^^^^^^^^^^^^ type: k.auth.SessionUser
        //       ^^^^^^^^^^^^^^^ type: corpus.AccountId
    }
}

page Home {
    @Inject let session: Session

    view {
        if let user = session.user {
        //            ^^^^^^^^^^^^ type: k.auth.SessionUser?
            text { "signed in" }
        }
    }
}
