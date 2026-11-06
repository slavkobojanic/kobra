// `.class([.panel, .expanded when isExpanded])` on a following line
func f() {
    box { content }
        .class([.panel, .expanded when isExpanded])
        .on(click: toggle)
}
