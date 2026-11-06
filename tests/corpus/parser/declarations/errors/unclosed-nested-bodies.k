// Every unclosed `{` reports K0101 at end of file.
service S {
    func f() {
        let x = 1
