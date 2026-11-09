/// The buttons, the state, the status line and the other tab's picture.
@Rendering(.client)
public component LabPanel {
    @Inject let lab: CallLab

    loading {
        paragraph { "loading" }
    }

    view {
        paragraph { "state: {lab.state}" }

        button { "Join" }
            .on(click: { await lab.join(relayed: false) })
        button { "Join relayed" }
            .on(click: { await lab.join(relayed: true) })
        button { "Send 640" }
            .on(click: { await lab.send(width: 640) })
        button { "Send 320" }
            .on(click: { await lab.send(width: 320) })
        button { "Stats" }
            .on(click: { await lab.measure() })
        button { "Hang up" }
            .on(click: { lab.hangUp() })

        paragraph { lab.status }
            .role(.status)

        if let remote = lab.remote {
            video(source: .stream(remote))
                .label("Remote")
        }
    }
}
