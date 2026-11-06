// A trailing lambda is the last argument when the last parameter is a function.
func repeatTimes(count: Int, body: (Int) -> Void) {}

func run() {
    repeatTimes(count: 3) { index =>
                          //^^^^^ type: Int
        let next = index + 1
    }
}
