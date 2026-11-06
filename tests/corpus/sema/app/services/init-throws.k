// A service's `init()` can't throw, because nothing could catch the error.
enum SetupError implements Error {
    case failed
}

service Setup {
    init() throws SetupError {
        throw SetupError.failed
    }
}
