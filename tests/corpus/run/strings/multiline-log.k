// A line break inside a logged message is escaped in `.stdout` (docs/compiler/testing.md §3.10).
import k.observe.Log

func main() {
    Log.warning("first\nsecond")
    Log.error("a backslash: \\")
}
