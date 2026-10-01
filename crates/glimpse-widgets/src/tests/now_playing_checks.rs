use super::*;

pub(super) fn check() {
    let playing = NowPlaying::new();
    let art = child_named::<gtk4::Image>(&playing, "now-playing__art");
    assert_eq!(
        (
            art.icon_name().as_deref(),
            art.has_css_class("now-playing__art--empty")
        ),
        (Some("audio-x-generic-symbolic"), true),
        "a player with no cover yet looks the same as one that lost its cover; the template \
             has to be born in the state set_art(None) would put it in, or the first frame is an \
             empty square nothing ever fills"
    );
    let source_icon = child_named::<gtk4::Image>(&playing, "now-playing__source-icon");
    let source_line = source_icon
        .parent()
        .and_downcast::<gtk4::Box>()
        .expect("source line");

    assert!(
        !source_line.get_visible(),
        "the line above the title is gone entirely until there is an application to name, \
             rather than holding open a gap the title then sits below"
    );
    playing.set_source(Some("Spotify"));
    assert!(source_line.get_visible());
    playing.set_icon_name(Some("audio-x-generic-symbolic"));
    assert!(source_icon.get_visible());
    playing.set_source(None::<&str>);
    assert!(
        source_line.get_visible(),
        "an icon alone still earns the line; it is emptiness of both that removes it"
    );
    playing.set_icon_name(None::<&str>);
    assert!(!source_line.get_visible());

    assert_eq!(
        playing.scrubber(),
        child_named::<Scrubber>(&playing, "scrubber")
    );
    assert_eq!(
        playing.transport(),
        child_named::<Transport>(&playing, "transport")
    );

    let empty = art.measure(gtk4::Orientation::Horizontal, -1).1;
    assert!(
        empty > 32,
        "the square is the stylesheet's, and a widget built before the providers were \
             installed never picks it up — GtkImage then measures its 16px default for every \
             case and the comparison below compares nothing"
    );
    let cover = gdk::MemoryTexture::new(
        192,
        192,
        gdk::MemoryFormat::R8g8b8a8,
        &glib::Bytes::from_owned(vec![0u8; 192 * 192 * 4]),
        192 * 4,
    );
    playing.set_art(Some(&cover));
    assert!(
        !art.has_css_class("now-playing__art--empty"),
        "real art drops the inset that makes the placeholder glyph sit small in its square"
    );
    assert_eq!(
        art.measure(gtk4::Orientation::Horizontal, -1).1,
        empty,
        "and it occupies exactly the square the placeholder held, so a cover arriving late \
             cannot resize the popover around it"
    );

    playing.set_art(None::<&gdk::Paintable>);
    assert_eq!(art.icon_name().as_deref(), Some("audio-x-generic-symbolic"));
    assert!(art.has_css_class("now-playing__art--empty"));
    assert_eq!(
        art.measure(gtk4::Orientation::Horizontal, -1).1,
        empty,
        "and the placeholder comes back to the same square, so losing a cover does not \
             resize the popover either"
    );
}
