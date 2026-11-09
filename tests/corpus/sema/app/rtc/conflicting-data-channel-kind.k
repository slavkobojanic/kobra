// A `DataChannelKind<T>` pairs a name with its message type (46-media-and-rtc.md, "Typed data
// channels"), and the two peers open a channel by its name, so two kinds named alike must carry the
// same type: the same K2408 as for `TabChannel`.
import k.rtc.DataChannelKind

struct ChatLine implements Codable {
    let text: String
}

let chatLines: DataChannelKind<ChatLine> = DataChannelKind(name: "chat")
let chatCounts: DataChannelKind<Int> = DataChannelKind(name: "chat")
