// An uncaught payload-free error has empty fields.
enum Denied implements Error, Codable {
    case forbidden
}

func main() throws Denied -> Int {
    throw Denied.forbidden
}
