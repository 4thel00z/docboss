/// How much of the input a diagnostic says was lost.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum Severity {
    /// Content was read with an approximation, such as a guessed encoding.
    Approximated,
    /// Content was skipped because it could not be read.
    Dropped,
}

/// One item the lenient reader approximated or dropped, reported instead of
/// silently lost.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Diagnostic {
    pub severity: Severity,
    /// The part, stream or structure the item came from, such as
    /// `word/document.xml` or `WordDocument`.
    pub location: String,
    pub message: String,
}

impl Diagnostic {
    pub fn dropped(location: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Dropped,
            location: location.into(),
            message: message.into(),
        }
    }

    pub fn approximated(location: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Approximated,
            location: location.into(),
            message: message.into(),
        }
    }
}
