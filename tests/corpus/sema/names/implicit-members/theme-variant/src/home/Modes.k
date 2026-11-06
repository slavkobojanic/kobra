// Generated cases resolve like declared ones; a variant the theme doesn't declare is K2006.
let night: ThemeVariant = .dark
//                         ^^^^ resolves: member ThemeVariant.dark
let day: ThemeVariant = .base
//                       ^^^^ resolves: member ThemeVariant.base
let other: ThemeVariant = .light
