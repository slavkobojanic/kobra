// Backslash escapes in literals; the stdout format writes a line break as `\n` and a backslash as `\\`.
import k.observe.Log

func main() -> List<String> {
    Log.info("line1\nline2")
    Log.info("back\\slash")
    Log.info("quote \" and brace {{}")
    return ["\u{41}\u{1F600}", "a\tb", "{{x}"]
}
