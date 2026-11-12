/// The payload of `StatementJob` (`storage.lock`).
public struct Statement implements Codable {
    let account: String
    let month: Int
}
