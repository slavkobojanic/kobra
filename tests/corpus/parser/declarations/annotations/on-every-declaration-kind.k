// Annotations may precede every kind of declaration and member.
@A component C {}
@A page P {}
@A layout L {}
@A service S {}
@A protocol Pr {}
@A entity En {}
@A repository R for En {}
@A struct St {}
@A enum Em {
    @B case one
}
@A class Cl {
    @B init() {}
}
@A interface I {}
@A theme T {}
@A app Ap {}
@A extend Int {}
@A test Suite {}
@A test "case" {}
@A func f() {}
@A let x = 1
@A protocol Q {
    @B rpc r()
}
