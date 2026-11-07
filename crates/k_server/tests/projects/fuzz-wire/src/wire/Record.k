/// A struct of every field shape.
public struct Record implements Codable {
    let id: Int
    let name: String
    let score: Double
    let active: Bool
    let note: String?
    let tags: List<String>
    let counts: Map<String, Int>
    let mood: Mood
    let shapes: List<Shape>
    let child: Leaf?
}
