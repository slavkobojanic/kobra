// A parameter shadows a member of the same name; `self.` still reaches the member.
struct Counter {
    let count: Int

    func plus(count: Int) -> Int {
        return count + self.count
        //     ^^^^^ resolves: param count 5:15
        //                  ^^^^^ resolves: member corpus.Counter.count
    }
}
