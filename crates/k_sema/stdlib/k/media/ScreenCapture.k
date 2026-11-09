/// What the screen picker offers (`MediaDevices.captureScreen`).
public struct ScreenCapture implements Equatable {
    /// Also capture the tab's or the system's sound, where the browser
    /// offers it.
    let audio: Bool = false
    /// Which kind of surface the picker offers first.
    let surface: DisplaySurface? = nil
}
