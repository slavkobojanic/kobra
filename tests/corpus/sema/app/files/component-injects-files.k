// `Files` is a platform interface, injectable only into services (K4005), like `LocalStorage`.
import k.device.Files

component Picker {
    @Inject let files: Files

    view {
        text { "pick" }
    }
}
