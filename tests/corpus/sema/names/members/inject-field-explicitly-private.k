// `private` on an `@Inject` field is redundant but allowed (k fmt removes it): no diagnostic.
import k.time.Clock

service Stamps {
    @Inject private let clock: Clock

    func now() -> Date {
        return clock.now
    }
}
