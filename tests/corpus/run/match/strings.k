// `match` on strings compares exactly, case included.
func command(text: String) -> Int {
    return match text {
        case "start" { 1 }
        case "stop" { 2 }
        case "" { 0 }
        default { -1 }
    }
}

func main() -> List<Int> {
    return [command(text: "start"), command(text: "stop"), command(text: "Start"), command(text: "")]
}
