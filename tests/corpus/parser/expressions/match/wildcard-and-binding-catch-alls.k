// `case _` and `case let other` are patterns; `default` is the keyword catch-all
func f() {
    match x {
        case _ { }
        case let other { use(other) }
        default { }
    }
}
