use super::document::{MarkId, Tool};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Interaction {
    Idle,
    Dragging { tool: Tool },
    Typing { id: MarkId },
    CropPending,
    Selected { id: MarkId },
    ListOpen,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EscOutcome {
    DiscardText,
    AbortDrag,
    DiscardCrop,
    Deselect,
    CloseList,
    Cancel,
}

pub fn esc(interaction: &Interaction) -> EscOutcome {
    match interaction {
        Interaction::Typing { .. } => EscOutcome::DiscardText,
        Interaction::Dragging { .. } => EscOutcome::AbortDrag,
        Interaction::CropPending => EscOutcome::DiscardCrop,
        Interaction::Selected { .. } => EscOutcome::Deselect,
        Interaction::ListOpen => EscOutcome::CloseList,
        Interaction::Idle => EscOutcome::Cancel,
    }
}

#[cfg(test)]
mod tests;
