/// How a data channel delivers its messages.
public enum Delivery implements Equatable {
    /// Ordered and retransmitted until delivered.
    case reliable
    /// In any order, retransmitted at most `retransmits` times: positions
    /// and other values where only the latest matters.
    case unordered(retransmits: Int)
}
