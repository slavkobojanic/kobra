// K1103: each import of the cycle has labels at the other imports, in order.
import b.B

public struct A {
    let b: B?
}
