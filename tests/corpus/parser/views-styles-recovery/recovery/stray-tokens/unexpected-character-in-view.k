// A non-ASCII character used as a name in a view is one Error token: K0001 only, wrapped in an ErrorNode.
component T {
    view {
        box { é }
    }
}
