import k.device.ShareButton

/// A button that shares the invite, or copies its link where the browser
/// can't share.
component ShareInvite {
    @Prop let invite: Invite

    @State var outcome: String = "nothing yet"

    view {
        section {
            ShareButton(content: .link(invite.url, title: "Join {invite.team}"), label: "Share invite")
                .on(shared: { outcome = "shared" })
                .on(copied: { outcome = "copied" })
                .on(failed: { failure =>
                    outcome = if failure == .denied { "refused" } else { "impossible" }
                })

            paragraph { "Outcome: {outcome}" }
        }
    }
}
