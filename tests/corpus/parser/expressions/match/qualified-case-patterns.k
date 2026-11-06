// case patterns may name the enum and its namespace
func f() {
    match s {
        case Status.active { }
        case shop.Status.archived(let at) { }
    }
}
