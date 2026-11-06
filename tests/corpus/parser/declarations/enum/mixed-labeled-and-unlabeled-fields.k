// Labeled and unlabeled case fields may be mixed.
enum Shape {
    case point(x: Int, Int)
    case named(String, size: Size)
}
