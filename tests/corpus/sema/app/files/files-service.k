// `Files` is a client-only platform interface (42-files.md): a service that injects it is
// client-only, `open` is async and throws `FileOpenError`, `save` throws `FileSaveError`, and each
// `catch` list is exhaustive over its enum.
import k.device.File
import k.device.FileOpenError
import k.device.FileType
import k.device.Files

enum Problem {
    case none
    case cancelled
    case tooLarge(limit: Int)
    case unreadable
    case notSaved
}

service Importer {
//      ^^^^^^^^ side: client-only
    @Inject let files: Files

    @State var text: String = ""
    @State var problem: Problem = .none

    func load() async {
        do {
            let file = try await files.open(accepting: [.json, .text])
            //  ^^^^ type: k.device.File
            text = file.text ?? ""
        } catch .cancelled {
            problem = .cancelled
        } catch .tooLarge(let limit) {
            problem = .tooLarge(limit: limit)
        } catch .unreadable {
            problem = .unreadable
        }
    }

    func loadAll() async {
        do {
            let chosen = try await files.openAll(accepting: [])
            //  ^^^^^^ type: List<k.device.File>
            text = chosen.map { f => f.name }.joined(separator: ", ")
        } catch let failure {
            problem = describe(failure: failure)
        }
    }

    func export() {
        let file = File.fromText(name: "notes.json", type: .json, text: text)
        do {
            try files.save(file)
        } catch .unavailable {
            problem = .notSaved
        }
    }

    func describe(failure: FileOpenError) -> Problem {
        return match failure {
            case .cancelled { .cancelled }
            case .tooLarge(let limit) { .tooLarge(limit: limit) }
            case .unreadable { .unreadable }
        }
    }

    func mediaType() -> String {
        return FileType.other(mediaType: "text/x-k", fileExtension: "k").mediaType
    }
}
