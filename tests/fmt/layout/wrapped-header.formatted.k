// Header clauses may be wrapped onto following lines.
public struct Matrix<
    Element,
    Size
>
    implements Equatable,
        Codable
    where Element implements Numeric,
        Size implements Dimension {
    let rows: Int
}
