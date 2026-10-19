//! Messages: conversations on the left, the open thread and the draft on the
//! right. The conversation list is virtual; the thread shows its newest
//! messages (a long history is cut to the last `THREAD_SHOWN`).

use cosmic::iced::{Alignment, Length};
use cosmic::widget::{self, Id};
use cosmic::{Element, theme};
use locus_core::view_model::{self, ConversationView, MessagesView, ThreadMessage};
use locus_core::{Msg, TextEdit};

use crate::view::{card, core, empty, row_area, scroll, tool, virtual_list};
use crate::{App, Message};

/// The conversation column beside the thread.
const LIST_W: f32 = 260.0;
/// Messages drawn in the thread, newest last.
const THREAD_SHOWN: usize = 300;
/// One conversation row: name, last message, and the time and unread count.
const ROW_H: f32 = 56.0;
/// The draft box's widget id.
const DRAFT_ID: &str = "message-draft";
const THREAD_SCROLL: &str = "scroll-thread";

pub(crate) fn messages(app: &App) -> Element<'_, Message> {
    let view = view_model::messages_view(&app.ui);
    let left = conversations(app, &view);
    let right = thread_pane(&view);
    widget::row::with_capacity(2)
        .spacing(theme::spacing().space_m)
        .height(Length::Fill)
        .push(left)
        .push(right)
        .into()
}

fn conversations<'a>(app: &'a App, view: &MessagesView) -> Element<'a, Message> {
    let list = if view.conversations.is_empty() {
        empty("No conversations", view.hint.to_string())
    } else {
        virtual_list(
            app,
            "scroll-conversations",
            view.conversations.len(),
            ROW_H,
            |i| conversation_row(i, &view.conversations[i]),
        )
    };
    widget::column::with_capacity(2)
        .spacing(theme::spacing().space_xs)
        .width(Length::Fixed(LIST_W))
        .height(Length::Fill)
        .push(tool("New conversation", "Ctrl+N", Some(Msg::New)))
        .push(list)
        .into()
}

fn conversation_row(i: usize, c: &ConversationView) -> Element<'static, Message> {
    let s = theme::spacing();
    let name = widget::column::with_capacity(2)
        .push(widget::text::body(view_model::short_ship(&c.peer)))
        .push(widget::text::caption(c.preview.clone()));
    let mut right = widget::column::with_capacity(2).align_x(Alignment::End);
    right = right.push(widget::text::caption(c.when.clone()));
    if c.unread > 0 {
        right = right.push(widget::text::caption(format!("{} new", c.unread)));
    }
    let content = widget::row::with_capacity(2)
        .spacing(s.space_xs)
        .align_y(Alignment::Center)
        .push(name.width(Length::Fill))
        .push(right);
    row_area(content, ROW_H, c.selected, Msg::Select(i), Msg::Activate)
}

fn thread_pane(view: &MessagesView) -> Element<'static, Message> {
    let s = theme::spacing();
    let Some(peer) = view.open.clone() else {
        return card(empty("Messages", view.hint.to_string()))
            .height(Length::Fill)
            .width(Length::Fill)
            .into();
    };
    let shown = &view.thread[view.thread.len().saturating_sub(THREAD_SHOWN)..];
    let mut bubbles = widget::column::with_capacity(shown.len() + 1).spacing(s.space_xs);
    if view.thread.len() > shown.len() {
        bubbles = bubbles.push(widget::text::caption(format!(
            "{} earlier messages are not shown",
            view.thread.len() - shown.len()
        )));
    }
    for m in shown {
        bubbles = bubbles.push(bubble(m));
    }
    let (block_label, block_tip) = if view.open_blocked {
        ("Unblock", "Accept messages from this ship again")
    } else {
        ("Block", "Refuse messages from this ship")
    };
    let block = tool(
        block_label,
        block_tip,
        Some(Msg::SetBlocked {
            peer: peer.clone(),
            blocked: !view.open_blocked,
        }),
    );
    let header = widget::row::with_capacity(3)
        .spacing(s.space_s)
        .align_y(Alignment::Center)
        .push(widget::text::title4(view_model::short_ship(&peer)).width(Length::Fill))
        .push(widget::text::caption(peer.clone()))
        .push(block);
    let draft = widget::text_input("Write a message", view.draft.clone())
        .id(Id::new(DRAFT_ID))
        .on_input(|value| core(Msg::Text(TextEdit::Set(value))))
        .on_focus(core(Msg::BeginDraft))
        .on_submit(|_| core(Msg::SendDraft))
        .width(Length::Fill);
    let send = widget::button::suggested("Send")
        .on_press_maybe(view.can_send.then(|| core(Msg::SendDraft)));
    let composer = widget::row::with_capacity(2)
        .spacing(s.space_xs)
        .align_y(Alignment::Center)
        .push(draft)
        .push(send);
    let mut column = widget::column::with_capacity(4)
        .spacing(s.space_s)
        .width(Length::Fill)
        .height(Length::Fill)
        .push(header);
    if view.open_blocked {
        column = column.push(widget::text::caption(
            "Blocked: messages from this ship are refused. Yours still go.",
        ));
    }
    column
        .push(scroll(THREAD_SCROLL, bubbles))
        .push(composer)
        .into()
}

/// One message: outgoing on the right, incoming on the left, with its time
/// and, for what this ship sent, its delivery status.
fn bubble(m: &ThreadMessage) -> Element<'static, Message> {
    let s = theme::spacing();
    let mut meta = widget::row::with_capacity(2).spacing(s.space_xs);
    meta = meta.push(widget::text::caption(m.when.clone()));
    if !m.status.is_empty() {
        meta = meta.push(widget::text::caption(m.status));
    }
    let text = widget::column::with_capacity(2)
        .spacing(s.space_xxxs)
        .push(widget::text::body(m.body.clone()))
        .push(meta);
    let bubble = widget::container(text)
        .padding(s.space_xs)
        .max_width(560.0)
        .class(theme::Container::Card);
    let row = if m.outgoing {
        widget::row::with_capacity(2)
            .push(widget::space::horizontal().width(Length::Fill))
            .push(bubble)
    } else {
        widget::row::with_capacity(2)
            .push(bubble)
            .push(widget::space::horizontal().width(Length::Fill))
    };
    row.width(Length::Fill).into()
}
