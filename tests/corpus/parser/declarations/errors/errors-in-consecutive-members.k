// Errors in consecutive members are reported independently.
service S {
    func () {}
    let : Int
    rpc r() throws
    func ok() {}
}
