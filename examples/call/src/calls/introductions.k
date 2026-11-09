import k.rtc.DataChannelKind

/// Names, sent once each way when the call connects.
public let introductions: DataChannelKind<Introduction> = DataChannelKind(name: "introductions")
