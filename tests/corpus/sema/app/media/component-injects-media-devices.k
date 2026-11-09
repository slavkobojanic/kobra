// `MediaDevices` is a platform interface, injectable only into services (K4005), like
// `Geolocation` (46-media-and-rtc.md, "Capture").
import k.media.MediaDevices

component RecordButton {
    @Inject let devices: MediaDevices

    view {
        text { "record" }
    }
}
