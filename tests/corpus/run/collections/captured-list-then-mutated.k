// A lambda reads the variable's current list, so it sees later appends; a copy taken earlier does not.
func main() -> List<Int> {
    var values = [1]
    let snapshot = values
    let size = { values.count }
    values.append(2)
    values.append(3)
    return [size(), snapshot.count]
}
