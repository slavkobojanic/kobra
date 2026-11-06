// `loading` must be followed by `{`; `loading() {}` is not a loading block and is skipped as K0126.
component T {
    loading() {}
    view {}
}
