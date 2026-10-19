//! Messages: a list of conversations with other ships, one open thread, and a
//! draft. Sending is optimistic: the message shows at once as pending, and the
//! next read of the thread shows what the ship and the peer's agent recorded.

use locus_client::messages::{Conversation, Message, Status};

use crate::msg::{Effect, Msg, Nav};
use crate::state::{App, Focus, Input, InputKind, Section, clamp_sel};
use crate::update::step;

/// `~zod`, `~tyr-mor`: a tilde and a lowercase name with hyphens.
pub fn valid_ship(raw: &str) -> bool {
    raw.len() > 1
        && raw.starts_with('~')
        && raw[1..]
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b == b'-')
}

/// Unread messages across all conversations.
pub fn unread_total(app: &App) -> u64 {
    app.messages.conversations.iter().map(|c| c.unread).sum()
}

/// The conversation row shown for `peer`, if any.
pub fn conversation<'a>(app: &'a App, peer: &str) -> Option<&'a Conversation> {
    app.messages.conversations.iter().find(|c| c.peer == peer)
}

/// The list arrived. A conversation open on screen is being read, so messages
/// that came into it since are marked read at once.
pub fn conversations_loaded(app: &mut App, fx: &mut Vec<Effect>, conversations: Vec<Conversation>) {
    app.messages.conversations = conversations;
    app.messages.listed = true;
    let i = Section::Messages.index();
    app.loading[i] = false;
    app.loaded[i] = true;
    // Keep the list cursor on the open conversation when it is listed.
    if let Some(open) = app.messages.open.clone()
        && let Some(pos) = app
            .messages
            .conversations
            .iter()
            .position(|c| c.peer == open)
    {
        app.messages.sel = pos;
    }
    app.messages.sel = clamp_sel(app.messages.sel, app.messages.conversations.len());
    if app.section == Section::Messages
        && let Some(open) = app.messages.open.clone()
        && let Some(c) = app
            .messages
            .conversations
            .iter_mut()
            .find(|c| c.peer == open && c.unread > 0)
    {
        c.unread = 0;
        fx.push(Effect::MarkRead { peer: open });
    }
}

pub fn thread_loaded(app: &mut App, peer: String, messages: Vec<Message>) {
    if app.messages.open.as_deref() != Some(peer.as_str()) {
        return;
    }
    let grew = messages.len() != app.messages.thread.len();
    app.messages.thread = messages;
    // A thread opens at its newest message; a reload keeps the cursor in place.
    if grew || app.messages.thread_sel >= app.messages.thread.len() {
        app.messages.thread_sel = app.messages.thread.len().saturating_sub(1);
    }
}

/// Open the thread with `peer`, read it, and clear its unread count here.
pub fn open_peer(app: &mut App, fx: &mut Vec<Effect>, peer: String) {
    let unread = conversation(app, &peer).map_or(0, |c| c.unread);
    if app.messages.open.as_deref() != Some(peer.as_str()) {
        app.messages.thread.clear();
        app.messages.thread_sel = 0;
    }
    app.messages.open = Some(peer.clone());
    fx.push(Effect::LoadThread { peer: peer.clone() });
    if unread > 0 {
        for c in app
            .messages
            .conversations
            .iter_mut()
            .filter(|c| c.peer == peer)
        {
            c.unread = 0;
        }
        fx.push(Effect::MarkRead { peer });
    }
}

pub fn select(app: &mut App, fx: &mut Vec<Effect>, i: usize) {
    app.messages.sel = clamp_sel(i, app.messages.conversations.len());
    if let Some(c) = app.messages.conversations.get(app.messages.sel) {
        let peer = c.peer.clone();
        open_peer(app, fx, peer);
        // A click on a conversation is a choice to write in it.
        begin_draft(app);
    }
}

/// Up and Down move the conversation list and open each one as it lands.
pub fn nav(app: &mut App, fx: &mut Vec<Effect>, nav: Nav) {
    let len = app.messages.conversations.len();
    let next = step(clamp_sel(app.messages.sel, len), len, nav);
    if len > 0 && next != app.messages.sel {
        select(app, fx, next);
    }
}

/// Up and Down move through the open thread (when it has the focus).
pub fn thread_nav(app: &mut App, nav: Nav) {
    let len = app.messages.thread.len();
    app.messages.thread_sel = step(app.messages.thread_sel, len, nav);
}

/// Enter in the list: open the selected conversation and go to its draft.
pub fn activate(app: &mut App, fx: &mut Vec<Effect>) {
    if app.messages.conversations.get(app.messages.sel).is_none() && app.messages.open.is_none() {
        begin_new(app);
        return;
    }
    if let Some(c) = app.messages.conversations.get(app.messages.sel) {
        let peer = c.peer.clone();
        open_peer(app, fx, peer);
    }
    begin_draft(app);
}

/// Put the caret in the draft (the shell calls this when its field is clicked).
pub fn begin_draft(app: &mut App) {
    if app.messages.open.is_none() {
        app.info("Choose a conversation, or start one with New");
        return;
    }
    app.focus = Focus::Input(InputKind::MessageDraft);
}

/// New: ask for the ship to write to.
pub fn begin_new(app: &mut App) {
    app.input = Input {
        text: "~".into(),
        target: None,
    };
    app.focus = Focus::Input(InputKind::NewPeer);
}

pub fn submit_new_peer(app: &mut App, fx: &mut Vec<Effect>) {
    let peer = app.input.text.trim().to_string();
    if !valid_ship(&peer) {
        app.error("Enter a ship name like ~zod");
        return;
    }
    app.input = Input::default();
    app.focus = Focus::List;
    open_peer(app, fx, peer);
    begin_draft(app);
}

/// Send the draft to the open conversation.
pub fn send(app: &mut App, fx: &mut Vec<Effect>) {
    let body = app.messages.draft.trim().to_string();
    if body.is_empty() {
        app.info("Nothing to send");
        return;
    }
    let Some(peer) = app.messages.open.clone() else {
        app.error("Choose a conversation first");
        return;
    };
    app.messages.draft.clear();
    app.messages.thread.push(Message {
        id: 0,
        peer: peer.clone(),
        outgoing: true,
        body: body.clone(),
        sent: String::new(),
        status: Status::Pending,
    });
    app.messages.thread_sel = app.messages.thread.len() - 1;
    fx.push(Effect::SendMessage { to: peer, body });
}

/// The peer's copy is recorded (or the send failed): read the thread again.
pub fn message_sent(app: &mut App, fx: &mut Vec<Effect>, peer: String) {
    if app.messages.open.as_deref() == Some(peer.as_str()) {
        fx.push(Effect::LoadThread { peer });
    }
    fx.push(Effect::LoadConversations);
}

/// The ship refused the message or could not be reached. The "sending" row
/// goes, and its text returns to the draft unless a new one was started.
pub fn send_failed(app: &mut App, peer: String, body: String, error: String) {
    if app.messages.open.as_deref() == Some(peer.as_str()) {
        if let Some(i) = app
            .messages
            .thread
            .iter()
            .rposition(|m| m.id == 0 && m.outgoing && m.body == body)
        {
            app.messages.thread.remove(i);
            app.messages.thread_sel = clamp_sel(app.messages.thread_sel, app.messages.thread.len());
        }
        if app.messages.draft.trim().is_empty() {
            app.messages.draft = body;
        }
    }
    app.error(format!("Message not sent: {error}"));
}

/// Quiet re-read of the list and the open thread, for the shell's timer.
pub fn poll(app: &App, fx: &mut Vec<Effect>) {
    if app.section != Section::Messages {
        return;
    }
    fx.push(Effect::LoadConversations);
    if let Some(peer) = app.messages.open.clone() {
        fx.push(Effect::LoadThread { peer });
    }
}

/// Delete asks first. A message still sending has no id yet.
pub fn ask_delete(app: &mut App) {
    let Some(m) = app.messages.thread.get(app.messages.thread_sel).cloned() else {
        app.info("No message selected");
        return;
    };
    if m.id == 0 {
        app.info("That message is still sending");
        return;
    }
    app.ask(
        "Delete this message from your copy? The other ship keeps theirs.",
        Msg::ExecuteDeleteMessage { id: m.id },
    );
}

pub fn execute_delete(app: &mut App, fx: &mut Vec<Effect>, id: u64) {
    fx.push(Effect::DeleteMessage { id });
    app.info("Deleting message");
}

pub fn message_deleted(app: &mut App, fx: &mut Vec<Effect>, id: u64) {
    app.messages.thread.retain(|m| m.id != id);
    app.messages.thread_sel = clamp_sel(app.messages.thread_sel, app.messages.thread.len());
    fx.push(Effect::LoadConversations);
    app.info("Message deleted");
}

/// Whether the listed conversation with `peer` refuses its incoming messages.
pub fn is_blocked(app: &App, peer: &str) -> bool {
    conversation(app, peer).is_some_and(|c| c.blocked)
}

/// Block or unblock `peer`. Nothing changes on screen until the ship confirms.
pub fn set_blocked(fx: &mut Vec<Effect>, peer: String, blocked: bool) {
    fx.push(Effect::SetBlocked { peer, blocked });
}

/// The ship confirmed the block: the listed conversation takes the new state,
/// and the list is read again.
pub fn block_changed(app: &mut App, fx: &mut Vec<Effect>, peer: String, blocked: bool) {
    if let Some(c) = app
        .messages
        .conversations
        .iter_mut()
        .find(|c| c.peer == peer)
    {
        c.blocked = blocked;
    }
    if blocked {
        app.info(format!("Blocked {peer}: its messages are refused"));
    } else {
        app.info(format!("Unblocked {peer}"));
    }
    fx.push(Effect::LoadConversations);
}

/// Whether the highlighted message is one this ship has recorded, so Delete
/// can act on it (a message still sending has no id yet).
pub fn thread_has_deletable(app: &App) -> bool {
    app.messages
        .thread
        .get(app.messages.thread_sel)
        .is_some_and(|m| m.id != 0)
}
