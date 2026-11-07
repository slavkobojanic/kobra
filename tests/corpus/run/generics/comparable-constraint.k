// Comparable constraints order any comparable type, strings and durations included.
func smallest<T>(_ a: T, _ b: T) -> T where T implements Comparable {
    return if a < b { a } else { b }
}

func main() -> List<String> {
    return ["{smallest(3, -1)}", smallest("pear", "apple"), "{smallest(2.minutes, 90.seconds).inSeconds}"]
}
