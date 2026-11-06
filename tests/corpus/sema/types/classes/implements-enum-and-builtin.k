// Enums and built-in value types are not interfaces either.
enum Kind {
    case a
}

struct Thing implements Kind {}
struct Other implements Int {}
