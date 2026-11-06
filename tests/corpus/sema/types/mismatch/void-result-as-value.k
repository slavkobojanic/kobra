// A function without a return type returns `Void`, which is not an `Int`.
func log() {}

func run() -> Int {
    let a = log()
      //^ type: Void
    let b: Int = log()
    return 0
}
