// Elements nest through trailing blocks; each level is a CallExpr with a LambdaExpr.
component T {
    view {
        section {
            article {
                box {
                    text { "deep" }
                }
            }
        }
    }
}
