import k.crypto.Uuid
import k.time.Clock

/// The call: a Join button, or how to unblock the camera; then both
/// pictures, the controls, the chat and the connection's statistics.
///
/// `@Rendering(.client)`: what it shows depends on the camera's permission
/// and the call, which only the browser knows, so the server renders its
/// `loading` block.
@Rendering(.client)
public component CallPanel {
    /// The room to join.
    @Prop let room: Uuid

    @Inject let call: CallService
    @Inject let clock: Clock

    @State var draft: String = ""

    /// Goes back to the camera when the browser's own "Stop sharing" button
    /// ended the screen.
    @Effect
    func followScreen() {
        if let shared = call.screen, not shared.isLive {
            call.stopSharing()
        }
    }

    /// Refreshes the statistics every 2 seconds while the page is shown.
    @Effect
    func followStats() async {
        while true {
            await call.refreshStats()
            await clock.sleep(2.seconds)
        }
    }

    func send() async {
        let text = draft

        draft = ""

        let sent = await call.say(text: text)
        if not sent {
            draft = text
        }
    }

    loading {
        paragraph { "Getting the call ready…" }
    }

    view {
        if not call.canCall {
            paragraph { "This browser can't make calls here. Open Call over https, or on localhost." }
                .role(.alert)
        } else if call.blocked {
            paragraph { "Camera or microphone access is blocked. Allow both in your browser's site settings to call." }
                .role(.alert)
        } else if not call.joined {
            button { "Join call" }
                .on(click: { await call.join(room: room) })
        }

        if let banner = call.banner {
            paragraph { banner }
                .role(.status)
        }

        if let problem = call.problem {
            paragraph { explain(problem: problem) }
                .role(.alert)
        }

        box {
            if let remote = call.remote {
                video(source: .stream(remote))
                    .label("{call.peerName ?? "The other person"}'s camera")
                    .class(.remote)
            }

            if let shared = call.screen {
                video(source: .stream(shared))
                    .label("Your screen")
                    .class(.mine)
            } else if let camera = call.camera {
                video(source: .stream(camera))
                    .label("Your camera")
                    .mirrored(true)
                    .class(.mine)
            }
        }
        .class(.pictures)

        if call.joined {
            box {
                button { if call.microphoneOn { "Mute" } else { "Unmute" } }
                    .on(click: { call.setMicrophone(on: not call.microphoneOn) })

                button { if call.cameraOn { "Turn camera off" } else { "Turn camera on" } }
                    .on(click: { call.setCamera(on: not call.cameraOn) })

                if call.cameras.count > 1 {
                    button { "Switch camera" }
                        .on(click: { await call.switchCamera() })
                }

                if call.screen == nil {
                    button { "Share screen" }
                        .on(click: { await call.shareScreen() })
                } else {
                    button { "Stop sharing" }
                        .on(click: { call.stopSharing() })
                }

                button { "Hang up" }
                    .on(click: { call.hangUp() })
            }
            .class(.controls)
        }

        if call.connection != nil {
            list {
                for entry in call.lines key entry.number {
                    item { "{entry.line.from}: {entry.line.text}" }
                }
            }
            .label("Chat")

            form {
                textField(value: bind draft)
                    .label("Message")

                button { "Send" }
                    .type(.submit)
            }
            .class(.composer)
            .on(submit: send)
        }

        if let stats = call.stats {
            list {
                item { "Round trip: {stats.roundTripTime?.inMilliseconds ?? 0} ms" }
                item { "Path: {if stats.isRelayed { "through the TURN relay" } else { "direct" }}" }
                item { "Packets lost: {stats.packetsLost}" }
            }
            .label("Connection")
            .class(.stats)
        }
    }

    /// A sentence explaining what went wrong.
    private func explain(problem: CallProblem) -> String {
        return match problem {
            case .devicesBlocked { "Camera or microphone access is blocked. Allow both in your browser's site settings to call." }

            case .noDevices { "No camera or microphone was found. Plug one in and try again." }

            case .devicesInUse { "Another application is using the camera or the microphone. Close it and try again." }

            case .cannotCapture { "The camera or the microphone couldn't be opened." }

            case .unsupported { "This browser can't make calls." }

            case .roomFull { "Two people are in this call already." }

            case .offline { "Couldn't reach Call. Check your connection and try again." }

            case .cannotShare { "The screen couldn't be shared." }
        }
    }

    style {
        .pictures {
            display: grid;
            gap: .sm;
        }

        .remote { width: 100%; backgroundColor: .foreground; }
        .mine { width: 12rem; }

        .controls {
            display: flex;
            gap: .sm;
        }

        .composer {
            display: flex;
            gap: .sm;
        }

        .stats { color: .muted; }
    }
}
