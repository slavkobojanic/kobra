// Outside a preview, a `testing` import is still test-only: each use is
// reported.
import testing.FakeCart

public component Summary {
    view {
        "{FakeCart.label}"
    }
}

preview {
    Summary()
}
