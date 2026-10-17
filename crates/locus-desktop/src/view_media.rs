//! Movies, Music and the player they share. The lists are virtual; the
//! picture is the newest decoded frame; the transport is the same bar in both
//! apps and in the photo viewer.

use cosmic::iced::{Alignment, ContentFit, Length};
use cosmic::widget;
use cosmic::{Element, theme};
use locus_core::Msg;
use locus_core::state::Section;
use locus_core::view_model::{self, MediaRow, PhotoDetail, PlayerView};

use crate::view::{ROW2_H, card, core, empty, empty_hint, row_area, virtual_list};
use crate::{App, Message};

/// The list column beside the picture.
const LIST_W: f32 = 340.0;

pub(crate) fn movies(app: &App) -> Element<'_, Message> {
    let rows = view_model::media_rows(&app.ui, Section::Movies);
    if rows.is_empty() {
        return empty(
            "No movies yet",
            empty_hint(app, "Click Upload to add video files to Movies."),
        );
    }
    let stage = stage(app, Section::Movies, &rows);
    let list = media_list(app, rows, Section::Movies);
    split(app, list, stage)
}

pub(crate) fn music(app: &App) -> Element<'_, Message> {
    let rows = view_model::media_rows(&app.ui, Section::Music);
    if rows.is_empty() {
        return empty(
            "No music yet",
            empty_hint(app, "Click Upload to add audio files to Music."),
        );
    }
    let info = now_card(&rows);
    let list = media_list(app, rows, Section::Music);
    split(app, list, info)
}

/// The list on the left, the picture or details on the right, and the
/// transport under both while a player has something loaded.
fn split<'a>(
    app: &'a App,
    list: Element<'a, Message>,
    side: Element<'a, Message>,
) -> Element<'a, Message> {
    let row = widget::row::with_capacity(2)
        .spacing(theme::spacing().space_m)
        .height(Length::Fill)
        .push(list)
        .push(side);
    with_transport(app, row.into())
}

/// The list: one row per file. Music rows show the artist and album beneath.
fn media_list(app: &App, rows: Vec<MediaRow>, section: Section) -> Element<'_, Message> {
    let count = rows.len();
    let list = virtual_list(app, list_id(section), count, ROW2_H, move |i| {
        let row = &rows[i];
        let line = widget::column::with_capacity(2)
            .push(widget::text::body(row.title.clone()))
            .push(widget::text::caption(row.subtitle.clone()));
        let mark = if row.playing { "• " } else { "" };
        let content = widget::row::with_capacity(2)
            .spacing(theme::spacing().space_xs)
            .align_y(Alignment::Center)
            .push(widget::text::caption(mark).width(Length::Fixed(16.0)))
            .push(line.width(Length::Fill))
            .push(widget::text::caption(row.size.clone()).width(Length::Fixed(72.0)));
        row_area(content, ROW2_H, row.selected, Msg::Select(i), Msg::Activate)
    });
    widget::container(list)
        .width(Length::Fixed(LIST_W))
        .height(Length::Fill)
        .into()
}

fn list_id(section: Section) -> &'static str {
    match section {
        Section::Music => "scroll-music",
        _ => "scroll-movies",
    }
}

/// Movies: the picture when a movie is playing, else the selected movie's
/// details.
fn stage<'a>(app: &'a App, section: Section, rows: &[MediaRow]) -> Element<'a, Message> {
    let playing_video = app
        .ui
        .player
        .as_ref()
        .is_some_and(|p| p.is_video && p.from == section);
    if playing_video && let Some(frame) = app.frame.clone() {
        return widget::container(
            widget::image(frame)
                .content_fit(ContentFit::Contain)
                .width(Length::Fill)
                .height(Length::Fill),
        )
        .center(Length::Fill)
        .into();
    }
    now_card(rows)
}

/// The selected item's name and folder, with Play. Music shows the same under
/// the album heading.
fn now_card<'a>(rows: &[MediaRow]) -> Element<'a, Message> {
    let s = theme::spacing();
    let selected = rows.iter().find(|r| r.selected).or(rows.first());
    let body: Element<'a, Message> = match selected {
        Some(row) => widget::column::with_capacity(4)
            .spacing(s.space_xxs)
            .push(widget::text::title3(row.title.clone()))
            .push(widget::text::caption(row.subtitle.clone()))
            .push(widget::text::caption(format!("Size {}", row.size)))
            .push(
                widget::button::suggested("Play")
                    .on_press(core(Msg::Activate))
                    .leading_icon(
                        widget::icon::from_name("media-playback-start-symbolic").handle(),
                    ),
            )
            .into(),
        None => widget::text::caption("Nothing selected").into(),
    };
    widget::container(card(body))
        .center_y(Length::Fill)
        .width(Length::Fill)
        .into()
}

/// Put the transport bar under `content` while a player has something loaded.
fn with_transport<'a>(app: &'a App, content: Element<'a, Message>) -> Element<'a, Message> {
    let s = theme::spacing();
    let mut col = widget::column::with_capacity(2)
        .spacing(s.space_s)
        .height(Length::Fill)
        .push(content);
    if let Some(view) = view_model::player(&app.ui) {
        col = col.push(transport(&view));
    }
    col.into()
}

/// The player bar: title, transport buttons, position, and volume.
pub(crate) fn transport(p: &PlayerView) -> Element<'static, Message> {
    let s = theme::spacing();
    let play_icon = if p.playing {
        "media-playback-pause-symbolic"
    } else {
        "media-playback-start-symbolic"
    };
    let title = widget::column::with_capacity(2)
        .push(widget::text::body(p.title.clone()))
        .push(widget::text::caption(match &p.error {
            Some(e) => format!("Could not play: {e}"),
            None => p.subtitle.clone(),
        }))
        .width(Length::Fill);
    let buttons = widget::row::with_capacity(4)
        .spacing(s.space_xxs)
        .align_y(Alignment::Center)
        .push(maybe_icon(
            "media-skip-backward-symbolic",
            "Previous",
            p.can_prev.then(|| Msg::PrevTrack),
        ))
        .push(icon_button_plain(
            play_icon,
            if p.playing { "Pause" } else { "Play" },
            Msg::TogglePlay,
        ))
        .push(maybe_icon(
            "media-skip-forward-symbolic",
            "Next",
            p.can_next.then(|| Msg::NextTrack),
        ))
        .push(icon_button_plain(
            "media-playback-stop-symbolic",
            "Stop",
            Msg::StopPlayback,
        ));
    let duration_ms = p.duration_ms.max(1) as f32;
    let seek = widget::slider(0.0..=1.0, p.fraction, move |f| {
        core(Msg::SeekTo((f * duration_ms) as u64))
    })
    .step(0.001_f32)
    .width(Length::Fill);
    let position = widget::row::with_capacity(3)
        .spacing(s.space_xxs)
        .align_y(Alignment::Center)
        .push(widget::text::caption(p.position.clone()))
        .push(seek)
        .push(widget::text::caption(p.duration.clone()));
    let volume = widget::row::with_capacity(2)
        .spacing(s.space_xxs)
        .align_y(Alignment::Center)
        .push(
            widget::icon::from_name("audio-volume-high-symbolic")
                .size(16)
                .icon(),
        )
        .push(
            widget::slider(0.0..=100.0, f32::from(p.volume), |v| {
                core(Msg::SetVolume(v as u8))
            })
            .width(Length::Fixed(120.0)),
        );
    let controls = widget::row::with_capacity(3)
        .spacing(s.space_m)
        .align_y(Alignment::Center)
        .push(title)
        .push(buttons)
        .push(volume);
    widget::container(
        widget::column::with_capacity(2)
            .spacing(s.space_xxs)
            .push(controls)
            .push(position),
    )
    .padding(s.space_s)
    .width(Length::Fill)
    .class(theme::Container::Card)
    .into()
}

fn maybe_icon(
    icon: &'static str,
    tip: &'static str,
    msg: Option<Msg>,
) -> Element<'static, Message> {
    let b = widget::button::icon(widget::icon::from_name(icon)).on_press_maybe(msg.map(core));
    widget::tooltip(
        b,
        widget::text::caption(tip),
        widget::tooltip::Position::Top,
    )
    .into()
}

fn icon_button_plain(icon: &'static str, tip: &'static str, msg: Msg) -> Element<'static, Message> {
    widget::tooltip(
        widget::button::icon(widget::icon::from_name(icon)).on_press(core(msg)),
        widget::text::caption(tip),
        widget::tooltip::Position::Top,
    )
    .into()
}

/// A video photo in the photo viewer: the picture, then the transport.
pub(crate) fn video_viewer<'a>(app: &'a App, detail: &PhotoDetail) -> Element<'a, Message> {
    let s = theme::spacing();
    let picture: Element<'a, Message> = match app.frame.clone() {
        Some(frame) if app.ui.player.as_ref().is_some_and(|p| p.is_video) => widget::container(
            widget::image(frame)
                .content_fit(ContentFit::Contain)
                .width(Length::Fill)
                .height(Length::Fill),
        )
        .center(Length::Fill)
        .into(),
        _ => widget::container(
            widget::column::with_capacity(2)
                .spacing(s.space_xxs)
                .align_x(Alignment::Center)
                .push(widget::text::title4(detail.title.clone()))
                .push(widget::text::caption("Press Play to watch this video.")),
        )
        .center(Length::Fill)
        .into(),
    };
    let mut col = widget::column::with_capacity(3)
        .spacing(s.space_s)
        .height(Length::Fill)
        .push(
            widget::row::with_capacity(2)
                .spacing(s.space_s)
                .push(widget::button::standard("Back to the grid").on_press(core(Msg::Escape)))
                .push(widget::button::suggested("Play").on_press(core(Msg::Activate)))
                .push(widget::text::caption(detail.meta.clone())),
        )
        .push(picture);
    if let Some(view) = view_model::player(&app.ui) {
        col = col.push(transport(&view));
    }
    col.into()
}
