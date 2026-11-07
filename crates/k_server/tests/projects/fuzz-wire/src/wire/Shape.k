/// An enum with payload cases: `{"case": …, label: …}`.
public enum Shape implements Codable {
    case point
    case circle(radius: Double)
    case box(width: Int, height: Int, label: String?)
}
