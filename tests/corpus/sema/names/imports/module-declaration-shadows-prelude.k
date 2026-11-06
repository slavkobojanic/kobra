// A module's own declaration shadows a prelude declaration of the same name.
struct Duration {
    let ticks: Int
}

func ticks(of duration: Duration) -> Int {
//                      ^^^^^^^^ resolves: decl corpus.Duration
    return duration.ticks
    //              ^^^^^ resolves: member corpus.Duration.ticks
}
