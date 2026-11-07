// Optionals compare with `nil` and with values of the wrapped type.
func main() -> List<Bool> {
    let some: Int? = 3
    let none: Int? = nil
    return [some == nil, none == nil, some != nil, some == 3, none == 3, none != 3]
}
