// Adding a Duration to a Date gives a Date; subtracting two Dates gives a Duration.
func main() -> List<Int> {
    let start = Date(unixSeconds: 1767268800)
    let later = start + 3.days
    let earlier = start - 2.hours
    let gap = later - earlier
    return [later.unixSeconds, earlier.unixSeconds, gap.inSeconds, (start - start).inSeconds]
}
