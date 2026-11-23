public theme SiteTheme {
    // The family's faces come from the library: this token uses them.
    font heading: Font = Font(family: "PP Neue Gstaad", size: 32px)
    // The library already has a bold upright face of this family.
    font display: Font = Font(
        family: "PP Neue Gstaad",
        size: 48px,
        faces: [FontFace("fonts/Display.woff2", weight: 700)]
    )
}
