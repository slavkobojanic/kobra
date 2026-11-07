// A trailing lambda fills the last function parameter, with or without other arguments.
func repeatTimes(_ count: Int, _ action: (Int) -> String) -> List<String> {
    var out: List<String> = []
    for i in 0..<count {
        out.append(action(i))
    }
    return out
}

func run(_ block: () -> String) -> String {
    return block()
}

func main() -> List<String> {
    return repeatTimes(2) { i => "#{i}" } + [run { "plain" }]
}
