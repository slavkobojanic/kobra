// Lengths are equal when their CSS text is equal; different units never are.
func main() -> List<Bool> {
    return [16px == 16px, 8px * 2 == 16px, 16px == 1rem, 16px != 1rem]
}
