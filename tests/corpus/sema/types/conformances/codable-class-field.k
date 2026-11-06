// A class field blocks `Codable`: classes don't derive it.
class Cache {
    var size: Int = 0
}

struct Snapshot implements Codable {
    let cache: Cache
}
