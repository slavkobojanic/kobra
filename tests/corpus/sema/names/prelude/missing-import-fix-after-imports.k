// With imports present, the fix adds the new import after them, keeping them sorted.
import k.http.HttpMethod

func method() -> HttpMethod {
    return .get
}

service Stamps {
    @Inject let clock: Clock
}
