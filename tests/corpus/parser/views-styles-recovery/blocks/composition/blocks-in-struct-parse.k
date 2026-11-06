// The parser accepts view, loading, failed and metadata blocks in any member list; rejecting them in a struct is semantic.
struct Point {
    view {}
    loading {}
    failed {}
    metadata {}
}
