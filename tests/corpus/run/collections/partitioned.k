// `partitioned` splits into the matching elements and the rest, both in order.
func main() -> List<List<Int>> {
    let parts = [1, 2, 3, 4, 5].partitioned { n => n % 2 == 0 }
    return match parts {
        case (let even, let odd) { [even, odd] }
    }
}
