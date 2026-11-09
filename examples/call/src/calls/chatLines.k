import k.rtc.DataChannelKind

/// The call's chat: typed lines, straight from one browser to the other.
public let chatLines: DataChannelKind<ChatLine> = DataChannelKind(name: "chat")
