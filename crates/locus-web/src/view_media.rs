//! Movies, Music, and the player they share. The lists are virtual; the
//! picture is the page's video element, placed over the stage; the transport
//! is the same bar in both apps and in the photo viewer.

use iced::widget::{button, column, container, image, row, slider, space, text};
use iced::{Background, Center, Color, ContentFit, Element, Fill, Length};
use locus_core::view_model::{self, MediaRow, PlayerView};
use locus_core::{Msg, Section};

use crate::style::{self, Colors};
use crate::view::{click_row, core, empty_card, muted_text, tool, virtual_rows};
use crate::{MEDIA_ROW_H, Message, Shell};

/// The list column beside the picture.
const LIST_W: f32 = 340.0;

pub(crate) fn movies(s: &Shell) -> Element<'_, Message> {
    library(s, Section::Movies)
}

pub(crate) fn music(s: &Shell) -> Element<'_, Message> {
    library(s, Section::Music)
}

fn library(s: &Shell, section: Section) -> Element<'_, Message> {
    let c = s.colors;
    let rows = view_model::media_rows(&s.app, section);
    if rows.is_empty() {
        let (title, hint) = match section {
            Section::Music => ("No music yet", "Click Upload to add audio files to Music."),
            _ => (
                "No movies yet",
                "Click Upload to add video files to Movies.",
            ),
        };
        return empty_card(c, title, hint);
    }
    let watching = s
        .app
        .player
        .as_ref()
        .is_some_and(|p| p.is_video && p.from == section);
    let side: Element<'_, Message> = if watching {
        stage(s.frame.as_ref())
    } else {
        now_card(c, &rows)
    };
    let list = media_list(s, section, rows);
    let body = row![list, side].spacing(16).height(Fill);
    let mut col = column![body].spacing(8).height(Fill);
    if let Some(view) = view_model::player(&s.app) {
        col = col.push(transport(c, &view));
    }
    col.into()
}

/// One row per file. Music rows carry the artist and album beneath the title.
fn media_list(s: &Shell, section: Section, rows: Vec<MediaRow>) -> Element<'_, Message> {
    let c = s.colors;
    let id = if section == Section::Music {
        "music"
    } else {
        "movies"
    };
    let count = rows.len();
    let list = virtual_rows(s, id, count, MEDIA_ROW_H, move |i| {
        media_row(c, &rows[i], i)
    });
    container(list)
        .width(Length::Fixed(LIST_W))
        .height(Fill)
        .into()
}

fn media_row(c: Colors, row: &MediaRow, i: usize) -> Element<'static, Message> {
    let mark = if row.playing { "•" } else { "" };
    let content = iced::widget::row![
        text(mark)
            .size(14)
            .color(c.accent)
            .width(Length::Fixed(16.0)),
        column![
            text(row.title.clone()).size(14).color(c.text),
            text(row.subtitle.clone()).size(12).color(c.muted),
        ]
        .spacing(2)
        .width(Fill),
        text(row.size.clone()).size(12).color(c.muted),
    ]
    .spacing(8)
    .align_y(Center);
    click_row(
        c,
        content,
        MEDIA_ROW_H,
        row.selected,
        Msg::Select(i),
        Msg::Activate,
    )
}

/// The selected item's name and folder, with Play.
fn now_card(c: Colors, rows: &[MediaRow]) -> Element<'static, Message> {
    let selected = rows.iter().find(|r| r.selected).or(rows.first());
    let body: Element<'static, Message> = match selected {
        Some(row) => column![
            text(row.title.clone()).size(20).color(c.text),
            muted_text(c, row.subtitle.clone()),
            muted_text(c, format!("Size {}", row.size)),
            play_button(c),
        ]
        .spacing(8)
        .into(),
        None => muted_text(c, "Nothing selected").into(),
    };
    container(body)
        .padding(16)
        .width(Fill)
        .height(Fill)
        .center_y(Fill)
        .style(move |_| style::card(c))
        .into()
}

fn play_button(c: Colors) -> Element<'static, Message> {
    button(text("Play").size(14))
        .padding([6, 14])
        .on_press(core(Msg::Activate))
        .style(style::primary_button(c))
        .into()
}

/// The video's current picture, centred in black. Until the first frame, black.
pub(crate) fn stage<'a>(frame: Option<&image::Handle>) -> Element<'a, Message> {
    let picture: Element<'a, Message> = match frame {
        Some(frame) => image(frame.clone())
            .content_fit(ContentFit::Contain)
            .width(Fill)
            .height(Fill)
            .into(),
        None => space::horizontal().into(),
    };
    container(picture)
        .width(Fill)
        .height(Fill)
        .center(Fill)
        .style(|_| container::Style {
            background: Some(Background::Color(Color::BLACK)),
            ..container::Style::default()
        })
        .into()
}

/// The picture of a video photo in the photo viewer: the stage while it plays,
/// else the title and Play.
pub(crate) fn video_picture<'a>(s: &Shell, title: &str) -> Element<'a, Message> {
    let c = s.colors;
    if s.app.player.as_ref().is_some_and(|p| p.is_video) {
        return stage(s.frame.as_ref());
    }
    container(
        column![
            text(title.to_string()).size(18).color(c.text),
            play_button(c),
        ]
        .spacing(12)
        .align_x(Center),
    )
    .center(Fill)
    .style(move |_| style::card(c))
    .into()
}

/// The player bar: title and error, transport buttons, position, and volume.
pub(crate) fn transport<'a>(c: Colors, p: &PlayerView) -> Element<'a, Message> {
    let play_label = if p.playing { "Pause" } else { "Play" };
    let controls = row![
        tool(c, "Previous", "", p.can_prev.then_some(Msg::PrevTrack)),
        button(text(play_label).size(14))
            .padding([6, 12])
            .on_press(core(Msg::TogglePlay))
            .style(style::primary_button(c)),
        tool(c, "Next", "", p.can_next.then_some(Msg::NextTrack)),
        button(text("Stop").size(14))
            .padding([6, 12])
            .on_press(core(Msg::StopPlayback))
            .style(style::quiet_button(c)),
    ]
    .spacing(6)
    .align_y(Center);
    let duration_ms = p.duration_ms.max(1) as f32;
    let seek = slider(0.0..=1.0, p.fraction, move |f| {
        core(Msg::SeekTo((f * duration_ms) as u64))
    })
    .step(0.001_f32)
    .width(Fill);
    let volume = slider(0.0..=100.0, f32::from(p.volume), |v| {
        core(Msg::SetVolume(v as u8))
    })
    .width(Length::Fixed(120.0));
    let caption = match &p.error {
        Some(e) => format!("Could not play: {e}"),
        None => p.subtitle.clone(),
    };
    let title = column![
        text(p.title.clone()).size(14).color(c.text),
        muted_text(c, caption),
    ]
    .spacing(2)
    .width(Fill);
    let top = row![title, controls, muted_text(c, "Volume"), volume]
        .spacing(12)
        .align_y(Center);
    let bottom = row![
        muted_text(c, p.position.clone()),
        seek,
        muted_text(c, p.duration.clone()),
    ]
    .spacing(8)
    .align_y(Center);
    container(column![top, bottom].spacing(8))
        .padding(12)
        .width(Fill)
        .style(move |_| style::card(c))
        .into()
}
