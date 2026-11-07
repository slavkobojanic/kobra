// `windowed` yields every run of `size` consecutive elements, none when the list is shorter.
func main() -> List<List<List<Int>>> {
    let values = [1, 2, 3, 4]
    return [values.windowed(2), values.windowed(4), values.windowed(5), values.windowed(1)]
}
