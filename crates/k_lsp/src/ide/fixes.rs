//! Quick fixes: the structured fixes of the compiler's diagnostics
//! (`docs/compiler/diagnostics.md`, "Fixes"), offered as code actions.

use k_diagnostics::{Applicability, Diagnostic, FileId, Fix, TextRange};
use k_sema::{Analysis, Cancelled};

/// One quick fix: the diagnostic it fixes and the fix.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuickFix {
    /// The diagnostic.
    pub diagnostic: Diagnostic,
    /// The fix.
    pub fix: Fix,
    /// `true` when the fix is the diagnostic's only machine-applicable one,
    /// so an editor may apply it on "fix all" or offer it first.
    pub preferred: bool,
}

/// The fixes of every diagnostic of `file` whose span overlaps `range` (or
/// touches it, for an empty range).
///
/// # Errors
///
/// [`Cancelled`] if the snapshot is stale.
pub fn quick_fixes(
    analysis: &Analysis,
    file: FileId,
    range: TextRange,
) -> Result<Vec<QuickFix>, Cancelled> {
    let mut out = Vec::new();
    for diagnostic in analysis.file_diagnostics(file)? {
        let span = diagnostic.span.range;
        let touches = span.intersect(range).is_some() || span.contains_range(range);
        if !touches || diagnostic.fixes.is_empty() {
            continue;
        }
        let machine = diagnostic
            .fixes
            .iter()
            .filter(|f| f.applicability == Applicability::MachineApplicable)
            .count();
        for fix in &diagnostic.fixes {
            out.push(QuickFix {
                diagnostic: diagnostic.clone(),
                fix: fix.clone(),
                preferred: machine == 1 && fix.applicability == Applicability::MachineApplicable,
            });
        }
    }
    Ok(out)
}
