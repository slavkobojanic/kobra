// The arguments of an initializer may span lines; the `(` must follow the name
// on the same line (rule N4), otherwise the operand is a type.
test Values {
    use Config(
        secret: Secret("s"),
        tolerance: 5.minutes
    ) for Config
}
