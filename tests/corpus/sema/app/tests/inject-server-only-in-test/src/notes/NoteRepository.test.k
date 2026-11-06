// A test group may inject server-only declarations; they come from the test's server container.
import k.auth.SessionManager

test NoteRepository {
    @Inject let notes: NoteRepository
    @Inject let sessions: SessionManager

    test "starts empty" {
        expect true
    }
}
