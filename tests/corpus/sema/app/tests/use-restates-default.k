// path: src/corpus/Defaults.test.k
// A `use` rule that restates a test default is a warning, with a fix that removes it.
import k.server.JobQueue
import k.storage.LocalStorage
import k.test.InlineJobQueue
import k.test.MemoryStorage
import k.test.TestClock
import k.time.Clock

test Defaults {
    use TestClock for Clock
    use MemoryStorage for LocalStorage
    use InlineJobQueue for JobQueue

    test "runs" {
        expect true
    }
}
