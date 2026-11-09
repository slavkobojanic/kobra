// Both peers open a data channel under an id derived from its name (46-media-and-rtc.md, "Typed
// data channels"), so two kinds whose names give the same id would be one channel: K2409. The same
// name twice, with the same type, is the same channel and is fine.
import k.rtc.DataChannelKind

struct Cursor implements Codable {
    let x: Int
    let y: Int
}

let al: DataChannelKind<Cursor> = DataChannelKind(name: "al")
let alAgain: DataChannelKind<Cursor> = DataChannelKind(name: "al")
let el: DataChannelKind<Cursor> = DataChannelKind(name: "el")
