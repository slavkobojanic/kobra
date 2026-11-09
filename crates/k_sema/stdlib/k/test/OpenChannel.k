/// A channel open on the test's connection ([`TestChannels.open`]).
public struct OpenChannel {
    /// The channel rpc's wire name, `/package.Protocol/Method`.
    let method: String
    /// The arguments it was opened with: the rpc's request message as
    /// JSON.
    let arguments: String
}
