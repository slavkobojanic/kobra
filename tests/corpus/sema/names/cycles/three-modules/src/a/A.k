// a -> b -> c -> a: every import of the cycle is K1103.
import b.B

public struct A {
    let b: B?
}
