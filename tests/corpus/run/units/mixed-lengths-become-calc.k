// Mixed units become `calc()`, percentages included.
func main() -> List<String> {
    let width = 100px + 2rem
    let fill = 100% - 2rem
    let tall = 50vh - 4em
    return ["{width}", "{fill}", "{tall}"]
}
