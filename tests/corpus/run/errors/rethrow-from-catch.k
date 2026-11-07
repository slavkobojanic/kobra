// A `catch` clause may throw a different error, which the enclosing `do` handles.
enum Low implements Error {
    case low
}

enum High implements Error {
    case high(String)
}

func low() throws Low {
    throw Low.low
}

func middle() throws High {
    do {
        try low()
    } catch .low {
        throw High.high("from low")
    }
}

func main() -> String {
    do {
        try middle()
        return "no error"
    } catch .high(let why) {
        return why
    }
}
