// Stdlib constraints apply too: `sortedBy` needs a `Comparable` key, and `Length` is not one.
struct Box {
    let width: Length
    let label: String
}

func sort(boxes: List<Box>) -> List<Box> {
    let byLabel = boxes.sortedBy { box => box.label }
    return boxes.sortedBy { box => box.width }
}
