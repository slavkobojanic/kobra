// Inside a method, a local shadows a member of the enclosing type; `self.` reaches the member.
struct Counter {
    let count: Int

    func doubled() -> Int {
        let count = self.count * 2
        //               ^^^^^ resolves: member corpus.Counter.count
        return count
        //     ^^^^^ resolves: local count 6:13
    }
}
