/// A payload-free enum, which crosses the wire as its case name.
public enum Availability implements Codable {
    case inStock
    case soldOut
}
