// `k.test` is test-only: production code can't import `TestClock`.
import k.test.TestClock

service Timer {
    @Inject let clock: TestClock
}
