// Lowercase calls in a view are built-in elements from the element table.
component Article {
    view {
        section {
      //^^^^^^^ resolves: element section
            heading(level: 2) { "Title" }
          //^^^^^^^ resolves: element heading
            paragraph { "Body" }
          //^^^^^^^^^ resolves: element paragraph
            box {
          //^^^ resolves: element box
                text { "inline" }
              //^^^^ resolves: element text
                emphasis { "stress" }
                lineBreak()
                code { "x" }
            }
            divider()
        }
    }
}
