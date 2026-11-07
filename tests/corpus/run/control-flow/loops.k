// for over ranges and lists, while, break and continue.
func main() -> List<Int> {
    var out: List<Int> = []
    for i in 0..<10 {
        if i % 2 == 0 {
            continue
        }
        if i > 7 {
            break
        }
        out.append(i)
    }
    var n = 1
    while n < 100 {
        n = n * 3
    }
    out.append(n)
    for value in [100, 200] {
        out.append(value)
    }
    return out
}
