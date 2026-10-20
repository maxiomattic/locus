//! Messages: conversations on the left, the open thread and the draft on the
//! right. The conversation list is virtual; a long thread is cut to its newest
//! messages.

use iced::alignment::Horizontal;
use iced::widget::{button, column, container, row, scrollable, space, text, text_input};
use iced::{Center, Element, Fill, Length};
use locus_core::view_model::{self, ConversationView, MessagesView, ThreadMessage};
use locus_core::{Msg, TextEdit};

use crate::style::{self, Colors};
use crate::view::{
    DRAFT_ID, click_row, core, empty_card, muted_text, scroll_msg, tool, virtual_rows,
};
use crate::{CONVERSATION_H, Message, Shell};

/// The conversation column beside the thread.
const LIST_W: f32 = 260.0;
/// Messages drawn in the thread, newest last.
const THREAD_SHOWN: usize = 300;
const THREAD_SCROLL: &str = "thread";

pub(crate) fn messages(s: &Shell) -> Element<'_, Message> {
    let MessagesView {
        conversations,
        open,
        open_label,
        open_group,
        thread,
        can_send,
        open_blocked,
        hint,
        ..
    } = view_model::messages_view(&s.app);
    row![
        conversations_column(s, conversations, hint),
        thread_pane(
            s,
            open,
            open_label,
            open_group,
            thread,
            can_send,
            open_blocked,
            hint
        ),
    ]
    .spacing(16)
    .height(Fill)
    .into()
}

fn conversations_column<'a>(
    s: &'a Shell,
    list: Vec<ConversationView>,
    hint: &'static str,
) -> Element<'a, Message> {
    let c = s.colors;
    let body: Element<'_, Message> = if list.is_empty() {
        empty_card(c, "No conversations", hint)
    } else {
        virtual_rows(s, "conversations", list.len(), CONVERSATION_H, move |i| {
            conversation_row(c, &list[i], i)
        })
    };
    column![
        tool(c, "New conversation", "Ctrl+N or Alt+N", Some(Msg::New)),
        tool(c, "New group", "", Some(Msg::BeginNewGroup)),
        body,
    ]
    .spacing(8)
    .width(Length::Fixed(LIST_W))
    .height(Fill)
    .into()
}

/// Name, last message, and the time and unread count.
fn conversation_row(c: Colors, v: &ConversationView, i: usize) -> Element<'static, Message> {
    let mut right = column![text(v.when.clone()).size(12).color(c.muted)]
        .spacing(2)
        .align_x(Horizontal::Right);
    if v.unread > 0 {
        right = right.push(text(format!("{} new", v.unread)).size(12).color(c.accent));
    }
    let content = row![
        column![
            text(if v.group {
                v.label.clone()
            } else {
                view_model::short_ship(&v.peer)
            })
            .size(14)
            .color(c.text),
            text(v.preview.clone()).size(12).color(c.muted),
        ]
        .spacing(2)
        .width(Fill),
        right,
    ]
    .spacing(8)
    .align_y(Center);
    click_row(
        c,
        content,
        CONVERSATION_H,
        v.selected,
        Msg::Select(i),
        Msg::Activate,
    )
}

fn thread_pane<'a>(
    s: &'a Shell,
    open: Option<String>,
    open_label: Option<String>,
    open_group: bool,
    thread: Vec<ThreadMessage>,
    can_send: bool,
    open_blocked: bool,
    hint: &'static str,
) -> Element<'a, Message> {
    let c = s.colors;
    let Some(peer) = open else {
        return empty_card(c, "Messages", hint);
    };
    let shown_from = thread.len().saturating_sub(THREAD_SHOWN);
    let mut bubbles = column![].spacing(8).width(Fill);
    if shown_from > 0 {
        bubbles = bubbles.push(muted_text(
            c,
            format!("{shown_from} earlier messages are not shown"),
        ));
    }
    for m in &thread[shown_from..] {
        bubbles = bubbles.push(bubble(c, m));
    }
    let title = open_label.unwrap_or_else(|| view_model::short_ship(&peer));
    let header = if open_group {
        row![
            text(title).size(18).color(c.text).width(Fill),
            muted_text(c, "group".to_string()),
        ]
    } else {
        row![
            text(title).size(18).color(c.text).width(Fill),
            muted_text(c, peer.clone()),
            block_button(c, peer.clone(), open_blocked),
        ]
    }
    .spacing(8)
    .align_y(Center);
    let mut pane = column![header].spacing(12).width(Fill).height(Fill);
    if open_blocked {
        pane = pane.push(muted_text(
            c,
            "Blocked: messages from this ship are refused. Yours still go.",
        ));
    }
    // Typing in the box makes it the draft, so the first key is never lost
    // after the click that focused it.
    let draft = text_input("Write a message", &s.app.messages.draft)
        .id(DRAFT_ID)
        .on_input(|value| {
            core(Msg::Batch(vec![
                Msg::BeginDraft,
                Msg::Text(TextEdit::Set(value)),
            ]))
        })
        .on_submit(core(Msg::SendDraft))
        .padding([6, 10])
        .width(Fill);
    let send = button(text("Send").size(14))
        .padding([6, 14])
        .style(style::primary_button(c))
        .on_press_maybe(can_send.then(|| core(Msg::SendDraft)));
    let composer = row![draft, send].spacing(8).align_y(Center);
    pane.push(
        scrollable(bubbles)
            .id(THREAD_SCROLL)
            .on_scroll(scroll_msg(THREAD_SCROLL))
            .height(Fill),
    )
    .push(composer)
    .into()
}

/// Blocks the open ship's messages, or lifts the block. Outgoing messages
/// still go either way.
fn block_button(c: Colors, peer: String, blocked: bool) -> Element<'static, Message> {
    let (label, tip) = if blocked {
        ("Unblock", "Accept messages from this ship again")
    } else {
        ("Block", "Refuse messages from this ship (yours still go)")
    };
    tool(
        c,
        label,
        tip,
        Some(Msg::SetBlocked {
            peer,
            blocked: !blocked,
        }),
    )
}

/// One message: outgoing on the right, incoming on the left, with its time
/// and, for what this ship sent, its delivery status.
fn bubble(c: Colors, m: &ThreadMessage) -> Element<'static, Message> {
    let mut meta = row![muted_text(c, m.when.clone())].spacing(8);
    if !m.status.is_empty() {
        meta = meta.push(muted_text(c, m.status));
    }
    let body = column![text(m.body.clone()).size(14).color(c.text), meta].spacing(4);
    let bubble = container(body)
        .padding([8, 12])
        .max_width(560)
        .style(move |_| style::card(c));
    let line = if m.outgoing {
        row![space::horizontal().width(Fill), bubble]
    } else {
        row![bubble, space::horizontal().width(Fill)]
    };
    line.width(Fill).into()
}
