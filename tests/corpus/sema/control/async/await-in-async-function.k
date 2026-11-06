// `await` on an async call inside an async function is fine, and the awaited
// call has the function's result type, not a future.
func fetchCount() async -> Int {
    return 3
}

func doubled() async -> Int {
//   ^^^^^^^ type: () async -> Int
    let count = await fetchCount()
//      ^^^^^ type: Int
//              ^^^^^^^^^^^^^^^^^^ type: Int
//                    ^^^^^^^^^^ resolves: decl corpus.fetchCount
    return count * 2
}
