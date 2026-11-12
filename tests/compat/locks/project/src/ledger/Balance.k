/// An account's balance (`rpc.lock`).
public struct Balance implements Codable {
    let account: String
    let cents: Int
    let note: String?
}
