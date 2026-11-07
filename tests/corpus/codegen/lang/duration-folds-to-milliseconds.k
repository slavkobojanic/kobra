// Constant durations fold to milliseconds (docs/compiler/codegen.md §5, §6.5).
func timeout() -> Duration {
    return 10.minutes + 30.seconds
}

func short() -> Duration {
    return 200ms
}
