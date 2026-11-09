import k.auth.Authenticated
import k.crypto.Uuid

/// One call's page, at its unguessable link.
public page Room requires Authenticated {
    /// The room's id, which its link carries.
    @Param let id: Uuid

    metadata {
        title("Call")
    }

    view {
        heading(level: 1) { "Call" }

        CallPanel(room: id)

        link(to: .lobby) { "Start another call" }
    }
}
