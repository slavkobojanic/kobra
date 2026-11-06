// An unterminated string in a metadata entry ends at the line end (K0002); the `(` left open is K0101 and the `}` on the next line still closes the block.
page P {
    metadata {
        title("Acme
    }
}
