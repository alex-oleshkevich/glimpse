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
