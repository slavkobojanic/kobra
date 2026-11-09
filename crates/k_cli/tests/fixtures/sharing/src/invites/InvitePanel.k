import k.device.CopyButton

/// The invite link with a button that copies it.
component InvitePanel {
    @Prop let invite: Invite

    view {
        section {
            paragraph { "Invite people to {invite.team}" }

            CopyButton(text: "{invite.url}", label: "Copy invite link")
        }
    }
}
