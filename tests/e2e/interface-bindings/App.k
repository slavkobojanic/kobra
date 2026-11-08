import greet.Greeter
import greet.LoudGreeter
import greet.QuietGreeter

/// Interface bindings: `Greeter` has two implementations, chosen per
/// environment; `Stamp` has one, so it needs no rule.
public app Bindings {
    metadata {
        title("Bindings")
    }

    use LoudGreeter for Greeter in .development
    use QuietGreeter for Greeter in .production, .test
}
