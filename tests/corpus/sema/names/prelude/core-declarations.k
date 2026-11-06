// Prelude declarations need no import and resolve by simple name.
func wait(for duration: Duration, items: List<String>) -> Int {
//                      ^^^^^^^^ resolves: decl Duration
//                                       ^^^^ resolves: decl List
//                                            ^^^^^^ resolves: decl String
    return items.count
}
