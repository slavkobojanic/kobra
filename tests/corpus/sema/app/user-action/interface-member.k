// An interface member whose implementation is user-action-only must be declared `@UserAction`, so
// calls through the interface are checked too (45-web-platform.md, "Function values"). The fix adds
// the annotation to the requirement.
import k.device.Files

interface Picking {
    func pick() async
}

service FilePicker implements Picking {
    @Inject let files: Files

    @State var name: String = ""

    func pick() async {
        do {
            name = try await files.open(accepting: []).name
        } catch _ {
            name = ""
        }
    }
}
