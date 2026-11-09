// The server may read a client-only `MediaDevices`' `isSupported` (`false`), its permissions
// (`.prompt`) and `devices` (empty) while it renders (45-web-platform.md, "Server rendering and the
// server"); calling `capture` there is K4012.
import k.device.PermissionState
import k.media.MediaDevice
import k.media.MediaDevices

service Microphones {
    @Inject let devices: MediaDevices

    @Derived let canRecord: Bool = devices.isSupported and devices.microphonePermission != .denied
    @Derived let camera: PermissionState = devices.cameraPermission
    @Derived let all: List<MediaDevice> = devices.devices
}

page Studio at "/studio" {
    @Inject let microphones: Microphones

    view {
        if microphones.canRecord {
            text { "{microphones.all.count} devices, camera {microphones.camera == .granted}" }
        }
    }
}
