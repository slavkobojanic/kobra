/// Mounting and unmounting, many times: crates/k_server/tests/memory.rs
/// runs the first as a warm-up and measures the heap around the second.
test Counter {
    test "mounts and unmounts 1,000 times" {
        for n in 1...1000 {
            let screen = render(Counter(label: "Counter {n}"))
            screen.button(named: "Add").click()
            expect screen.text("Count: 1").exists
            screen.unmount()
        }
    }

    test "mounts and unmounts 10,000 times" {
        for n in 1...10000 {
            let screen = render(Counter(label: "Counter {n}"))
            screen.button(named: "Add").click()
            expect screen.text("Count: 1").exists
            screen.unmount()
        }
    }
}
