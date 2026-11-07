// Each enum exports a constructor object: frozen payload-free singletons and payload functions with
// labelled fields; unlabelled payloads are `_0`, `_1` (docs/compiler/codegen.md §5).
enum Shape {
    case empty
    case circle(radius: Int)
    case pair(Int, Int)
}

func shapes() -> List<Shape> {
    return [.empty, .circle(radius: 2), .pair(1, 2)]
}
