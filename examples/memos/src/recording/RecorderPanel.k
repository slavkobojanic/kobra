import k.core.Binding
import k.media.LevelMeter
import memos.clockText

/// The recorder: a Record button, or how to unblock the microphone; while
/// recording, the level, the time, Pause and Stop.
///
/// `@Rendering(.client)`: what it shows depends on the microphone's
/// permission and devices, which only the browser knows, so the server
/// renders its `loading` block.
@Rendering(.client)
public component RecorderPanel {
    @Inject let recorder: MemoRecorder

    /// Stores a recording that stopped by itself: the microphone was
    /// unplugged, or the memo reached three minutes.
    @Effect
    func finishWhenStopped() async {
        if recorder.stoppedBy() != nil {
            await recorder.stop()
        }
    }

    loading {
        paragraph { "Getting the recorder ready…" }
    }

    view {
        if not recorder.canRecord {
            paragraph { "This browser can't record here. Open Memos over https, or on localhost." }
                .role(.alert)
        } else if recorder.microphone == .denied {
            paragraph { "Microphone access is blocked. Allow it in your browser's site settings to record." }
                .role(.alert)
        } else if let session = recorder.session {
            if let meter = recorder.meter {
                LevelMeter(meter: meter, label: "Microphone level")
            }

            paragraph { "Recording {clockOf(session.elapsed)}" }

            box {
                button {
                    if session.state == .paused {
                        "Resume"
                    } else {
                        "Pause"
                    }
                }
                .on(click: { recorder.togglePause() })

                button { "Stop" }
                    .on(click: { await recorder.stop() })
            }
            .class(.controls)
        } else {
            if recorder.microphones.count > 1 {
                dropdown(selection: Binding(get: { recorder.chosen?.value ?? "" }, set: { id => recorder.choose(id: id) })) {
                    option(value: "") { "Default microphone" }

                    for device in recorder.microphones key device.id.value {
                        option(value: device.id.value) { device.label ?? "Microphone" }
                    }
                }
                .label("Microphone")
            }

            button { "Record" }
                .on(click: { await recorder.start() })
                .class(.record)
        }

        // A blocked microphone is explained above already.
        if let problem = recorder.problem, recorder.microphone != .denied {
            paragraph { explain(problem: problem) }
                .role(.alert)
        }

        if let notice = recorder.notice {
            paragraph { notice }
                .role(.status)
        }
    }

    /// The elapsed time as `m:ss`.
    private func clockOf(_ elapsed: Duration) -> String {
        return clockText(elapsed)
    }

    /// A sentence explaining why there is no new memo.
    private func explain(problem: MemoProblem) -> String {
        return match problem {
            case .microphoneBlocked { "Microphone access is blocked. Allow it in your browser's site settings to record." }

            case .noMicrophone { "No microphone was found. Plug one in and try again." }

            case .microphoneInUse { "Another application is using the microphone. Close it and try again." }

            case .cannotCapture { "The microphone couldn't be opened." }

            case .cannotRecord { "The recording failed. Try again." }

            case .notSaved { "The memo couldn't be saved. Check your connection and try again." }
        }
    }

    style {
        .controls {
            display: flex;
            gap: .sm;
        }

        .record { justifySelf: start; }
    }
}
