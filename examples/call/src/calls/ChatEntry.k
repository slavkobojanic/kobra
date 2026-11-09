/// A chat line as the page lists it, numbered in arrival order.
public struct ChatEntry implements Equatable {
    /// 1 for the first line of the call.
    let number: Int
    /// The line.
    let line: ChatLine
}
