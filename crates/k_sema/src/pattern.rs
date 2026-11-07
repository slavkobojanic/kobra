//! Exhaustiveness and reachability of `match` arms and `catch` clauses
//! (sema.md §11.1): Maranget's usefulness algorithm over a pattern matrix.
//!
//! Constructors are enum cases (with their payloads), `true`/`false`, an
//! optional's `nil` and some value, and a tuple (the single constructor of
//! its type, whose fields are the elements, so `(0, true)` covers only the
//! values it names). Literal and range patterns have no
//! finite constructor set: they never make a match exhaustive. A pattern
//! vector is *useful* with respect to a matrix when some value matches it
//! and no row of the matrix; an arm is unreachable when its pattern isn't
//! useful, and a match is exhaustive when the wildcard isn't.
//!
//! The algorithm specializes the matrix per constructor, so its cost is
//! linear in the number of rows times the pattern depth in practice. Two
//! things keep it linear for enums with thousands of cases (the
//! `huge-match` benchmark family, `docs/compiler/performance.md`):
//!
//! * a constructor's field types are computed from that constructor alone
//!   ([`arity`]), never by enumerating every case of the enum, and
//! * a wildcard column is specialized by grouping the rows by their head
//!   constructor once ([`useful_wild`]), not by scanning every row for
//!   every constructor.
//!
//! Callers that test one arm after another against a growing matrix keep
//! it in [`Rows`], which indexes the rows by head pattern column after
//! column, so each arm is tested against only the rows that can overlap it,
//! also when many arms share a constructor (`case .c(1)`, `case .c(2)`).

use std::collections::{HashMap, HashSet};

use crate::body::Checker;
use crate::cx::apply;
use crate::program::{GenId, MemberId};
use crate::ty::Ty;

/// A constructor of a pattern.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub(crate) enum Ctor {
    /// An enum case.
    Case(MemberId),
    /// `true`.
    True,
    /// `false`.
    False,
    /// An optional's `nil`.
    Nil,
    /// An optional's value.
    Some,
    /// A tuple of this many elements: its type's only constructor.
    Tuple(usize),
}

/// A checked pattern.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) enum Pat {
    /// Matches everything (`_`, `let x`, `default`).
    Wild,
    /// A constructor with sub-patterns for its fields.
    Ctor(Ctor, Vec<Pat>),
    /// A literal, compared by its text.
    Lit(String),
    /// A range: never covers anything for exhaustiveness.
    Range,
}

/// The constructors of `ty` with their field types; `None` when the type
/// can't be enumerated.
fn ctors(checker: &Checker<'_, '_, '_>, ty: &Ty) -> Option<Vec<(Ctor, Vec<Ty>)>> {
    let cx = checker.cx_ref();
    let ty = checker.resolve(ty);
    match &ty {
        Ty::Tuple(ts) => Some(vec![(Ctor::Tuple(ts.len()), ts.to_vec())]),
        Ty::Optional(inner) => Some(vec![
            (Ctor::Nil, Vec::new()),
            (Ctor::Some, vec![(**inner).clone()]),
        ]),
        Ty::Named(d, args) => {
            if Some(*d) == cx.b.bool_ {
                return Some(vec![(Ctor::True, Vec::new()), (Ctor::False, Vec::new())]);
            }
            if !cx.is_enum(*d) {
                return None;
            }
            let decl = cx.p.decl(*d);
            let subst: Vec<(GenId, Ty)> = decl
                .generics
                .iter()
                .copied()
                .zip(args.iter().cloned())
                .collect();
            Some(
                cx.cases(*d)
                    .into_iter()
                    .map(|m| {
                        let sig = cx.case_sig(m);
                        (
                            Ctor::Case(m),
                            sig.params.iter().map(|p| apply(&p.ty, &subst)).collect(),
                        )
                    })
                    .collect(),
            )
        }
        _ => None,
    }
}

/// The field types of `ctor` as a constructor of `ty`; empty when `ctor`
/// has no fields or isn't a constructor of `ty`. Constant time in the
/// number of `ty`'s constructors.
fn arity(checker: &Checker<'_, '_, '_>, ctor: &Ctor, ty: &Ty) -> Vec<Ty> {
    let cx = checker.cx_ref();
    match (ctor, checker.resolve(ty)) {
        (Ctor::Some, Ty::Optional(inner)) => vec![(*inner).clone()],
        (Ctor::Tuple(n), Ty::Tuple(ts)) if ts.len() == *n => ts.to_vec(),
        (Ctor::Case(m), Ty::Named(d, args)) => {
            if !cx.is_enum(d) || cx.p.member(*m).owner != d {
                return Vec::new();
            }
            let decl = cx.p.decl(d);
            let subst: Vec<(GenId, Ty)> = decl
                .generics
                .iter()
                .copied()
                .zip(args.iter().cloned())
                .collect();
            cx.case_sig(*m)
                .params
                .iter()
                .map(|p| apply(&p.ty, &subst))
                .collect()
        }
        _ => Vec::new(),
    }
}

/// A growing pattern matrix, indexed by head pattern column after column.
///
/// [`Rows::useful`] answers the same as [`useful_depth`] over [`Rows::rows`]. The
/// rows are kept in a trie: each node holds the rows of its matrix, and
/// splits them by the head of their first column into one child per
/// constructor and per literal (holding the *specialized* rows: the head's
/// sub-patterns followed by the rest), plus the rows with a wildcard head.
///
/// While a node has no wildcard-headed row, specializing its matrix by a
/// constructor or a literal is exactly that child's matrix, so a row with a
/// constructor or literal head is tested against the child alone, and so on
/// down the columns. Testing `n` arms that differ anywhere (`case .a`,
/// `case .b`, or `case .c(1)`, `case .c(2)`, …) is then linear in `n`
/// instead of quadratic. A row with a wildcard head, or a node that has
/// wildcard-headed rows, falls back to [`useful_depth`] over that node's rows,
/// which is linear in their number; such rows are rare (`default`, a
/// catch-all, `.c(let v)`), and every later row of the same group is
/// usually unreachable.
///
/// Memory is the rows times their depth: a row is stored once per level of
/// the trie it reaches.
#[derive(Clone, Debug, Default)]
pub(crate) struct Rows {
    /// The node of the whole matrix (depth 0).
    root: Node,
}

/// One node of the [`Rows`] trie: the matrix of the rows that reach it.
#[derive(Clone, Debug, Default)]
struct Node {
    /// Every row of this node's matrix, in insertion order.
    rows: Vec<Vec<Pat>>,
    /// The rows with a constructor head, specialized by it.
    by_ctor: HashMap<Ctor, Node>,
    /// The rows with a literal head, specialized by it.
    by_lit: HashMap<String, Node>,
    /// The number of rows with a wildcard head. While it is zero, the
    /// children are the node's specialized matrices.
    wild: usize,
    /// `true` once a row reached the depth limit here: the children are
    /// incomplete and [`useful_depth`] must use `rows`.
    flat: bool,
}

/// The depth beyond which [`useful_depth`] gives up (answers `false`), and
/// [`Rows`] stops indexing.
const MAX_DEPTH: u32 = 64;

impl Rows {
    /// An empty matrix.
    pub(crate) fn new() -> Rows {
        Rows::default()
    }

    /// Appends `row`, whose columns have types `tys` (the types give the
    /// constructors' arities when the row is indexed).
    pub(crate) fn push(&mut self, checker: &Checker<'_, '_, '_>, row: Vec<Pat>, tys: &[Ty]) {
        self.root.push(checker, row, tys, 0);
    }

    /// The rows, in insertion order.
    pub(crate) fn rows(&self) -> &[Vec<Pat>] {
        &self.root.rows
    }

    /// Returns `true` if `row` matches some value that no row matches;
    /// `tys` are the column types. Same result as [`useful_depth`] over
    /// [`Rows::rows`].
    pub(crate) fn useful(&self, checker: &Checker<'_, '_, '_>, row: &[Pat], tys: &[Ty]) -> bool {
        self.root.useful(checker, row, tys, 0)
    }

    /// The top-level patterns that no row covers, rendered as K source
    /// (`.amber`, `.loaded(_)`, `true`, `nil`), for the notes and fixes of
    /// K2301 and K2206; `["_"]` when the column's type has no finite set of
    /// constructors (or only a nested pattern is missing, which a wildcard
    /// arm also covers). Empty when the rows are exhaustive.
    ///
    /// Each constructor is tested once against the index, so the cost is
    /// linear in the type's constructors.
    pub(crate) fn missing(&self, checker: &Checker<'_, '_, '_>, ty: &Ty) -> Vec<String> {
        if !self.useful(checker, &[Pat::Wild], std::slice::from_ref(ty)) {
            return Vec::new();
        }
        // A tuple's only constructor is spelled like a wildcard of its
        // elements: what is missing is nested, and `_` covers it.
        let Some(all) = ctors(checker, ty)
            .filter(|all| !all.is_empty() && !matches!(all[..], [(Ctor::Tuple(_), _)]))
        else {
            return vec!["_".to_owned()];
        };
        let cx = checker.cx_ref();
        let mut out = Vec::new();
        for (c, fields) in all {
            let row = [Pat::Ctor(c.clone(), vec![Pat::Wild; fields.len()])];
            if !self.useful(checker, &row, std::slice::from_ref(ty)) {
                continue;
            }
            let wilds = vec!["_"; fields.len()].join(", ");
            out.push(match c {
                Ctor::Case(m) if fields.is_empty() => format!(".{}", cx.p.member(m).name),
                Ctor::Case(m) => format!(".{}({wilds})", cx.p.member(m).name),
                Ctor::True => "true".to_owned(),
                Ctor::False => "false".to_owned(),
                Ctor::Nil => "nil".to_owned(),
                Ctor::Some => "let value".to_owned(),
                Ctor::Tuple(_) => format!("({wilds})"),
            });
        }
        if out.is_empty() {
            // Only a nested pattern is missing: a wildcard covers it.
            out.push("_".to_owned());
        }
        out
    }
}

impl Node {
    /// Adds `row` (columns typed by `tys`) to this node at trie depth
    /// `depth`, and to the child matrix its head specializes into; a row
    /// past [`MAX_DEPTH`] marks the node [`flat`](Node::flat) instead.
    fn push(&mut self, checker: &Checker<'_, '_, '_>, row: Vec<Pat>, tys: &[Ty], depth: u32) {
        let index = !self.flat && depth <= MAX_DEPTH;
        if !index {
            self.flat = true;
            self.rows.push(row);
            return;
        }
        if let Some((head, rest)) = row.split_first() {
            let ty = tys.first().cloned().unwrap_or(Ty::Error);
            let rest_tys = tys.get(1..).unwrap_or(&[]);
            match head {
                Pat::Ctor(c, subs) => {
                    let fields = arity(checker, c, &ty);
                    let mut v = subs.clone();
                    v.resize(fields.len(), Pat::Wild);
                    v.extend_from_slice(rest);
                    let mut t = fields;
                    t.extend_from_slice(rest_tys);
                    self.by_ctor
                        .entry(c.clone())
                        .or_default()
                        .push(checker, v, &t, depth + 1);
                }
                Pat::Lit(text) => {
                    self.by_lit.entry(text.clone()).or_default().push(
                        checker,
                        rest.to_vec(),
                        rest_tys,
                        depth + 1,
                    );
                }
                Pat::Wild => self.wild += 1,
                // A range never covers anything (see `useful_depth`).
                Pat::Range => {}
            }
        }
        self.rows.push(row);
    }

    /// Returns `true` if `row` matches a value no row of this node's
    /// matrix matches: follows the index while the node has no wildcard
    /// rows and is not flat, and falls back to [`useful_depth`] otherwise.
    fn useful(&self, checker: &Checker<'_, '_, '_>, row: &[Pat], tys: &[Ty], depth: u32) -> bool {
        let Some((head, rest)) = row.split_first() else {
            return self.rows.is_empty();
        };
        if depth > MAX_DEPTH {
            return false;
        }
        if self.flat || self.wild > 0 {
            return useful_depth(checker, &self.rows, row, tys, depth);
        }
        let ty = tys.first().cloned().unwrap_or(Ty::Error);
        let rest_tys = tys.get(1..).unwrap_or(&[]);
        match head {
            Pat::Ctor(c, subs) => {
                let fields = arity(checker, c, &ty);
                let mut v = subs.clone();
                v.resize(fields.len(), Pat::Wild);
                v.extend_from_slice(rest);
                let mut t = fields;
                t.extend_from_slice(rest_tys);
                match self.by_ctor.get(c) {
                    Some(child) => child.useful(checker, &v, &t, depth + 1),
                    None => useful_depth(checker, &[], &v, &t, depth + 1),
                }
            }
            Pat::Lit(text) => match self.by_lit.get(text) {
                Some(child) => child.useful(checker, rest, rest_tys, depth + 1),
                None => useful_depth(checker, &[], rest, rest_tys, depth + 1),
            },
            // No wildcard-headed rows: nothing covers a range's values.
            Pat::Range => useful_depth(checker, &[], rest, rest_tys, depth + 1),
            Pat::Wild => useful_depth(checker, &self.rows, row, tys, depth),
        }
    }
}

/// Merges two ascending index lists into one ascending sequence.
fn merge<'a>(a: &'a [usize], b: &'a [usize]) -> impl Iterator<Item = usize> + 'a {
    let (mut i, mut j) = (0, 0);
    std::iter::from_fn(move || {
        let next = match (a.get(i), b.get(j)) {
            (Some(&x), Some(&y)) if x <= y => {
                i += 1;
                x
            }
            (_, Some(&y)) => {
                j += 1;
                y
            }
            (Some(&x), None) => {
                i += 1;
                x
            }
            (None, None) => return None,
        };
        Some(next)
    })
}

/// Rows of `rows` specialized by `ctor` (whose fields have types `fields`).
fn specialize(rows: &[Vec<Pat>], ctor: &Ctor, fields: usize) -> Vec<Vec<Pat>> {
    let mut out = Vec::new();
    for row in rows {
        let Some((head, rest)) = row.split_first() else {
            continue;
        };
        match head {
            Pat::Wild => {
                let mut r = vec![Pat::Wild; fields];
                r.extend_from_slice(rest);
                out.push(r);
            }
            Pat::Ctor(c, subs) if c == ctor => {
                let mut r = subs.clone();
                r.resize(fields, Pat::Wild);
                r.extend_from_slice(rest);
                out.push(r);
            }
            _ => {}
        }
    }
    out
}

/// Returns `true` if `row` matches some value that no row of `rows`
/// matches; `tys` are the column types (Maranget's usefulness). `depth`
/// counts the specializations so far: past [`MAX_DEPTH`] the answer is
/// `false`, so pathological nesting can't recurse without bound.
fn useful_depth(
    checker: &Checker<'_, '_, '_>,
    rows: &[Vec<Pat>],
    row: &[Pat],
    tys: &[Ty],
    depth: u32,
) -> bool {
    let Some((head, rest)) = row.split_first() else {
        return rows.is_empty();
    };
    if depth > MAX_DEPTH {
        return false;
    }
    let ty = tys.first().cloned().unwrap_or(Ty::Error);
    let rest_tys = tys.get(1..).unwrap_or(&[]);
    match head {
        Pat::Ctor(c, subs) => {
            let fields = arity(checker, c, &ty);
            let spec = specialize(rows, c, fields.len());
            let mut v = subs.clone();
            v.resize(fields.len(), Pat::Wild);
            v.extend_from_slice(rest);
            let mut t = fields;
            t.extend_from_slice(rest_tys);
            useful_depth(checker, &spec, &v, &t, depth + 1)
        }
        Pat::Lit(text) => {
            let spec: Vec<Vec<Pat>> = rows
                .iter()
                .filter_map(|r| {
                    let (h, rr) = r.split_first()?;
                    match h {
                        Pat::Wild => Some(rr.to_vec()),
                        Pat::Lit(t) if t == text => Some(rr.to_vec()),
                        _ => None,
                    }
                })
                .collect();
            useful_depth(checker, &spec, rest, rest_tys, depth + 1)
        }
        Pat::Range => {
            let spec: Vec<Vec<Pat>> = rows
                .iter()
                .filter_map(|r| {
                    let (h, rr) = r.split_first()?;
                    matches!(h, Pat::Wild).then(|| rr.to_vec())
                })
                .collect();
            useful_depth(checker, &spec, rest, rest_tys, depth + 1)
        }
        Pat::Wild => useful_wild(checker, rows, rest, &ty, rest_tys, depth),
    }
}

/// [`useful_depth`] for a row whose head is a wildcard (`rest` is the
/// row's tail, `ty` the head column's type). When the rows' heads cover
/// every constructor of `ty`, the wildcard is useful if it is useful under
/// some constructor; the rows are grouped by head constructor once, so the
/// cost is linear in the rows plus the constructors. Otherwise only the
/// rows with a wildcard head matter (the default matrix).
fn useful_wild(
    checker: &Checker<'_, '_, '_>,
    rows: &[Vec<Pat>],
    rest: &[Pat],
    ty: &Ty,
    rest_tys: &[Ty],
    depth: u32,
) -> bool {
    let heads: HashSet<&Ctor> = rows
        .iter()
        .filter_map(|r| match r.first() {
            Some(Pat::Ctor(c, _)) => Some(c),
            _ => None,
        })
        .collect();
    if let Some(all) = ctors(checker, ty).filter(|all| !all.is_empty()) {
        if all.iter().all(|(c, _)| heads.contains(c)) {
            let mut by_head: HashMap<&Ctor, Vec<usize>> = HashMap::new();
            let mut wild = Vec::new();
            for (i, r) in rows.iter().enumerate() {
                match r.first() {
                    Some(Pat::Ctor(c, _)) => by_head.entry(c).or_default().push(i),
                    Some(Pat::Wild) => wild.push(i),
                    _ => {}
                }
            }
            return all.into_iter().any(|(c, fields)| {
                let same = by_head.get(&c).map_or(&[][..], Vec::as_slice);
                let group: Vec<Vec<Pat>> = merge(same, &wild).map(|i| rows[i].clone()).collect();
                let spec = specialize(&group, &c, fields.len());
                let mut v = vec![Pat::Wild; fields.len()];
                v.extend_from_slice(rest);
                let mut t = fields;
                t.extend_from_slice(rest_tys);
                useful_depth(checker, &spec, &v, &t, depth + 1)
            });
        }
    }
    let default: Vec<Vec<Pat>> = rows
        .iter()
        .filter_map(|r| {
            let (h, rr) = r.split_first()?;
            matches!(h, Pat::Wild).then(|| rr.to_vec())
        })
        .collect();
    useful_depth(checker, &default, rest, rest_tys, depth + 1)
}
