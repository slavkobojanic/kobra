// `button` already has a role, so it defines no `.role` (K3103).
component Pay {
    view {
        button { "Pay" }
            .role(.status)
    }
}
