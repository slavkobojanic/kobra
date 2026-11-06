// Case fields may span lines and end with a trailing comma.
enum Event {
    case moved(
        from: Point,
        to: Point,
    )
    case resized(Size,)
}
