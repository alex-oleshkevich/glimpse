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
mod tests {
    use super::*;

    #[test]
    fn the_esc_chain_maps_every_interaction() {
        assert_eq!(esc(&Interaction::Typing { id: 1 }), EscOutcome::DiscardText);
        assert_eq!(
            esc(&Interaction::Dragging { tool: Tool::Arrow }),
            EscOutcome::AbortDrag
        );
        assert_eq!(esc(&Interaction::CropPending), EscOutcome::DiscardCrop);
        assert_eq!(esc(&Interaction::Selected { id: 1 }), EscOutcome::Deselect);
        assert_eq!(esc(&Interaction::ListOpen), EscOutcome::CloseList);
        assert_eq!(esc(&Interaction::Idle), EscOutcome::Cancel);
    }
}
