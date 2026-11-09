/// What arrives on the call's data channels, merged into one stream.
public enum CallEvent {
    /// A chat line from the other person.
    case line(ChatLine)
    /// The other person said who they are.
    case introduced(Introduction)
}
