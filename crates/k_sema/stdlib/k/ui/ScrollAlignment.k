/// Where [`ElementRef.scrollIntoView(alignment:animated:)`] puts the
/// element, on the block axis (vertically, in a page of lines).
public enum ScrollAlignment {
    /// Its start at the start of the scrolling box.
    case start
    /// Its middle in the middle.
    case center
    /// Its end at the end.
    case end
    /// Whichever scrolls least; nothing when it is already in view.
    case nearest
}
