func f() {
    guard let x = try g() else { return }
    guard try h() else { return }
}
