// Stdlib declarations outside the prelude are imported by their module path.
import k.time.Clock

service Stamps {
    @Inject let clock: Clock
    //                 ^^^^^ resolves: decl k.time.Clock

    func stamp() -> Date {
        return clock.now
        //     ^^^^^ resolves: member corpus.Stamps.clock
        //           ^^^ resolves: member k.time.Clock.now
    }
}
