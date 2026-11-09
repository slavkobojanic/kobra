// `KeepScreenAwake` (45-web-platform.md, "KeepScreenAwake") renders nothing and keeps the screen
// on while it is mounted, so the condition is the `if` around it. Its `failed` event carries a
// `WakeLockFailure`, whose two cases a `match` must both handle. It has no props (K2004).
import k.device.KeepScreenAwake
import k.device.WakeLockFailure

component RecipeSteps {
    @Prop let stepByStep: Bool

    @State var note: String = ""

    func noteScreenMaySleep(_ reason: WakeLockFailure) {
        note = match reason {
            case .unsupported { "This browser can't keep the screen on." }
            case .unavailable { "The screen may turn off." }
        }
    }

    view {
        if stepByStep {
            KeepScreenAwake()
                .on(failed: { reason => noteScreenMaySleep(reason) })
        }

        KeepScreenAwake(seconds: 10)

        text { note }
    }
}
