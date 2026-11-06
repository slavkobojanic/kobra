// An unknown parameter type is K1202 only: member accesses on the parameter report nothing.
func size(widget: Widgt) -> Int {
    return widget.size + widget.width.count
}
