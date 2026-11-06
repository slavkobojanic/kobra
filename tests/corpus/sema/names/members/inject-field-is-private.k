// `@Inject` fields are always private: reading another service's dependency is K1301, and `.now` reports nothing more.
import k.time.Clock

service Stamps {
    @Inject let clock: Clock
}

service Reports {
    @Inject let stamps: Stamps

    func stampedAt() -> Date {
        return stamps.clock.now
    }
}
