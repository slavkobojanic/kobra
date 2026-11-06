// b's test file imports a: test files take no part in cycle detection.
import a.A

test "an A holds a B" {
    expect A(b: B(id: 1)).b.id == 1
}
