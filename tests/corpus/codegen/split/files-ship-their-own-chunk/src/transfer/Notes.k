import k.device.File
import k.device.Files

public service Notes {
    @Inject let files: Files

    @State var text: String = ""

    func load() async {
        do {
            let file = try await files.open(accepting: [.text])
            text = file.text ?? ""
        } catch .cancelled {
            text = ""
        } catch .tooLarge(let limit) {
            text = "over {limit} bytes"
        } catch .unreadable {
            text = ""
        }
    }

    func export() {
        do {
            try files.save(File.fromText(name: "notes.txt", type: .text, text: text))
        } catch .unavailable {
            text = ""
        }
    }
}
