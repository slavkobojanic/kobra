// iterables can be closed ranges and calls with arguments
func f() {
    for age in 13...120 { }
    for x in xs.filter(isNew) { }
    for i in 0..<xs.count { }
}
