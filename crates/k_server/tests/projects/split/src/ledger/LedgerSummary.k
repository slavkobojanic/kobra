/// Client-safe: crosses the wire, so it is in both bundles.
public struct LedgerSummary implements Codable {
    let title: String
    let total: Int
}
