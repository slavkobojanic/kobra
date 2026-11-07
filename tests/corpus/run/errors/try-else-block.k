// `try … else` runs its block only on an error; the block may contain statements before its value.
import k.observe.Log

enum Miss implements Error {
    case miss
}

func lookup(key: String) throws Miss -> Int {
    if key == "k" {
        return 7
    }
    throw Miss.miss
}

func main() -> List<Int> {
    let hit = try lookup(key: "k") else {
        Log.info("not reached")
        0
    }
    let missed = try lookup(key: "z") else {
        Log.info("falling back")
        -1
    }
    return [hit, missed]
}
