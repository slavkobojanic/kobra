// `@Inject` is not allowed on a struct field: plain types inject nothing.
import k.time.Clock

struct Stamp {
    @Inject let clock: Clock
}
