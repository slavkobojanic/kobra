// Trimming removes leading and trailing whitespace of every kind; case mapping covers all letters.
func main() -> List<String> {
    return ["  hi  ".trimmed(), "MiXed".lowercased(), "MiXed".uppercased(), "\t x \n".trimmed(), "   ".trimmed()]
}
