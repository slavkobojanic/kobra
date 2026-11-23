// A preview without its body: the parser expects `{` and still makes a
// `PreviewDecl` of the keyword and the name, so the declaration keeps its
// place in the file for the checker and the editor's outline.
preview "no body"
