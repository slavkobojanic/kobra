// An operand that cannot start a type but can start an expression is an `Expr`;
// semantic analysis decides whether it is allowed.
test Values {
    use .sample for Config
    use 42 for Config
    use makeConfig() for Config
}
