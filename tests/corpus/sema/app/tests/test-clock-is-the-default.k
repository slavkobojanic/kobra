// path: src/corpus/Timing.test.k
// In tests, `Clock` is `TestClock` without a `use` rule, reached by its concrete type.
import k.test.TestClock

test Timing {
    @Inject let clock: TestClock
    //                 ^^^^^^^^^ resolves: decl k.test.TestClock

    test "time moves only when advanced" {
        let start = clock.now
        //          ^^^^^^^^^ type: Date
        clock.advance(by: 5.minutes)
        expect clock.now - start == 5.minutes
    }
}
