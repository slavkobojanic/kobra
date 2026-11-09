import k.device.KeepScreenAwake
import k.device.WakeLockFailure

/// A recipe whose step-by-step mode keeps the screen on.
public component Recipe {
    @State var stepByStep: Bool = false
    @State var note: String = ""

    func noteScreenMaySleep(_ reason: WakeLockFailure) {
        note = match reason {
            case .unsupported { "This browser can't keep the screen on." }
            case .unavailable { "The screen may turn off." }
        }
    }

    view {
        button { "Step by step" }
            .on(click: { stepByStep = not stepByStep })

        if stepByStep {
            KeepScreenAwake()
                .on(failed: { reason => noteScreenMaySleep(reason) })
        }

        text { note }
    }
}
