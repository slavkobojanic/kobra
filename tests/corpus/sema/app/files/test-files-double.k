// path: src/corpus/Importer.test.k
// In `k test`, `TestFiles` is the default `Files`: a test injects it by its concrete type to say what
// the user chooses and to read what was saved (42-files.md, "Tests").
import k.device.File
import k.device.Files
import k.device.FileType
import k.test.TestFiles

service Exporter {
    @Inject let files: Files

    func export(text: String) {
        do {
            try files.save(File.fromText(name: "out.txt", type: .text, text: text))
        } catch .unavailable {
        }
    }
}

test Exporter {
    @Inject let testFiles: TestFiles
    @Inject let exporter: Exporter

    test "saves a download" {
        exporter.export(text: "hello")

        expect testFiles.saved.count == 1
        expect testFiles.saved[0].text == "hello"
        expect testFiles.saved[0].mediaType == FileType.text.mediaType
    }

    test "the user's choice is answered" {
        testFiles.choose(File.fromText(name: "in.json", type: .json, text: "[]"))
        testFiles.failSaves(error: .unavailable)

        expect testFiles.requests.isEmpty
    }
}
