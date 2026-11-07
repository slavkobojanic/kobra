/// A request the fake API recorded (`GET /_fake/requests`).
public struct FakeRequest implements Codable {
    let method: String
    let path: String
    let headers: Map<String, String>
    let body: String
}
