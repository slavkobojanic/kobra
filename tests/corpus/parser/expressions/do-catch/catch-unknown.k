// `catch unknown` is the catch-all for cases added after the clauses
// were written; `unknown` is a keyword only right before the block
func f() {
    do { try a() } catch .x { } catch unknown { log() }
}
