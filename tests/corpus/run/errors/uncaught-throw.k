// An error `main` throws is the case's result, encoded with its type and payload.
enum Problem implements Error, Codable {
    case outOfStock(available: Int)
}

func main() throws Problem -> Int {
    throw Problem.outOfStock(available: 3)
}
