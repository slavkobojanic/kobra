// `File` and `FileType` (42-files.md) in both runtimes: `File.fromText` encodes UTF-8 with the type's
// media type, `text` decodes it, `size` counts bytes, `File.maximumSize` is 16 MiB, and each
// `FileType` has its media type and extension (`.other` its own).
import k.device.File
import k.device.FileType

func main() -> List<String> {
    let file = File.fromText(name: "notes.txt", type: .text, text: "héllo")
    let other = FileType.other(mediaType: "text/x-k", fileExtension: "k")
    return [file.mediaType, file.text ?? "nil", "{file.size}", "{File.maximumSize}", FileType.json.mediaType, other.fileExtension]
}
