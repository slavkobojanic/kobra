// A trailing lambda for a last parameter that isn't a function is K2001 at the lambda.
func repeatTimes(body: (Int) -> Void, count: Int) {}

func run() {
    repeatTimes(body: { index => index }) { 3 }
}
