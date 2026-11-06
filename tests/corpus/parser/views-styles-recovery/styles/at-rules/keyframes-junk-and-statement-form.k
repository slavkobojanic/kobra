// Something that is not a keyframe selector reports K0100 and is skipped up to
// the next selector; `@keyframes x;` has no block (K0100 at the `;`).
component T {
    style {
        @keyframes fade {
            .a
            to { opacity: 1; }
        }
        @keyframes x;
    }
}
