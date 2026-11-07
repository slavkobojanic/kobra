// Ints interpolate as plain decimal digits, negative ones with a leading `-`.
func main() -> List<String> {
    let big = 9007199254740991
    let negative = -42
    return ["{big}", "{negative}", "{0}", "n={negative + 2}"]
}
