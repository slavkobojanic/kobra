// `&&` and `||` in a view condition are K0104 and K0105, treated as `and` and `or`.
component T {
    view {
        if a && b || c {
            Ok()
        }
    }
}
