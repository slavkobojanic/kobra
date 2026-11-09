/// The buttons and the status line.
@Rendering(.client)
public component LabPanel {
    @Inject let lab: CaptureLab

    loading {
        paragraph { "loading" }
    }

    view {
        paragraph {
            if lab.isSupported {
                "supported"
            } else {
                "unsupported"
            }
        }

        button { "Ask" }
            .on(click: { await lab.ask() })
        button { "Capture" }
            .on(click: { await lab.capture() })
        button { "Capture missing" }
            .on(click: { await lab.captureMissing() })
        button { "Record voice" }
            .on(click: { await lab.record(format: .voice) })
        button { "Record audio" }
            .on(click: { await lab.record(format: .audio) })
        button { "Record video" }
            .on(click: { await lab.record(format: .video) })

        paragraph { lab.status }
            .role(.status)

        if let recording = lab.recording {
            video(source: .file(recording.file))
                .captions(.notNeeded)
                .label("Playback")
        }
    }
}
