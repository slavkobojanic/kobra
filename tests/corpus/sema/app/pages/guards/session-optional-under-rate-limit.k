// A page guarded only by `RateLimit` is open to guests, so `session.user` stays optional.
import k.auth.AuthUserId
import k.auth.RateLimit
import k.auth.Session
import k.crypto.Uuid

struct AccountId implements AuthUserId {
    let value: Uuid
}

page Limited requires RateLimit(10.perMinute) {
    @Inject let session: Session

    view {
        if let user = session.user {
        //            ^^^^^^^^^^^^ type: k.auth.SessionUser?
            text { "signed in" }
        }
    }
}
