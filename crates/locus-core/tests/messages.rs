//! Messenger and folder moves in Files: what the core asks the shell to do,
//! and how the state follows.

mod common;

use common::*;
use locus_client::messages::{Conversation, Message, Status};
use locus_core::messages::valid_ship;
use locus_core::{Effect, Focus, InputKind, Section, view_model};

fn msg(id: u64, peer: &str, outgoing: bool, body: &str, status: Status) -> Message {
    Message {
        id,
        peer: peer.into(),
        outgoing,
        body: body.into(),
        sent: "~2026.10.17..09.00.00..0000".into(),
        status,
    }
}

fn conv(peer: &str, unread: u64, last: Message) -> Conversation {
    Conversation {
        peer: peer.into(),
        count: 2,
        unread,
        last: Some(last),
    }
}

/// Messages open with two conversations; `~tyr` has two unread messages.
fn inbox() -> App {
    let mut app = App::new();
    send(&mut app, Msg::Switch(Section::Messages));
    send(
        &mut app,
        Msg::ConversationsLoaded {
            conversations: vec![
                conv(
                    "~dev",
                    0,
                    msg(3, "~dev", false, "see you at six", Status::Received),
                ),
                conv(
                    "~tyr",
                    2,
                    msg(9, "~tyr", false, "photos are up", Status::Received),
                ),
            ],
        },
    );
    app
}

fn sent_to(fx: &[Effect]) -> Vec<(String, String)> {
    fx.iter()
        .filter_map(|e| match e {
            Effect::SendMessage { to, body } => Some((to.clone(), body.clone())),
            _ => None,
        })
        .collect()
}

#[test]
fn ship_names_are_tilde_and_lowercase_with_hyphens() {
    assert!(valid_ship("~zod"));
    assert!(valid_ship("~tyr-mor"));
    assert!(!valid_ship("~"));
    assert!(!valid_ship("zod"));
    assert!(!valid_ship("~Zod"));
    assert!(!valid_ship("~zod.x"));
}

#[test]
fn opening_a_conversation_reads_its_thread_and_clears_unread_here() {
    let mut app = inbox();
    let fx = send(&mut app, Msg::Select(1));
    assert!(fx.contains(&Effect::LoadThread {
        peer: "~tyr".into()
    }));
    assert!(fx.contains(&Effect::MarkRead {
        peer: "~tyr".into()
    }));
    assert_eq!(app.messages.open.as_deref(), Some("~tyr"));
    let view = view_model::messages_view(&app);
    assert_eq!(view.unread_total, 0);
    assert!(view.conversations[1].open);
}

#[test]
fn a_read_conversation_is_not_marked_again() {
    let mut app = inbox();
    let fx = send(&mut app, Msg::Select(0));
    assert_eq!(
        fx,
        vec![Effect::LoadThread {
            peer: "~dev".into()
        }]
    );
}

#[test]
fn sending_is_optimistic_and_asks_the_shell_to_send() {
    let mut app = inbox();
    send(&mut app, Msg::Select(0));
    send(&mut app, Msg::BeginDraft);
    assert_eq!(app.focus, Focus::Input(InputKind::MessageDraft));
    type_str(&mut app, "  on my way  ");
    let fx = send(&mut app, Msg::Submit);
    assert_eq!(sent_to(&fx), vec![("~dev".into(), "on my way".into())]);
    let view = view_model::messages_view(&app);
    assert!(view.draft.is_empty());
    let last = view.thread.last().expect("optimistic message");
    assert!(last.outgoing);
    assert_eq!(last.status, "Sending");
    assert!(!view.can_send, "nothing left to send");
}

#[test]
fn an_empty_draft_sends_nothing() {
    let mut app = inbox();
    send(&mut app, Msg::Select(0));
    send(&mut app, Msg::BeginDraft);
    type_str(&mut app, "   ");
    let fx = send(&mut app, Msg::Submit);
    assert!(sent_to(&fx).is_empty());
    assert_eq!(app.status.text, "Nothing to send");
}

#[test]
fn send_draft_sends_whatever_field_has_focus() {
    // Enter and the Send button both use SendDraft, so sending must not
    // depend on the draft being the focused input.
    let mut app = inbox();
    send(&mut app, Msg::Select(0));
    app.focus = Focus::List;
    app.messages.draft = "from the button".into();
    let fx = send(&mut app, Msg::SendDraft);
    assert_eq!(
        sent_to(&fx),
        vec![("~dev".to_string(), "from the button".to_string())]
    );
    assert!(app.messages.draft.is_empty());
}

#[test]
fn sending_without_a_conversation_is_refused() {
    let mut app = inbox();
    app.messages.draft = "hello".into();
    app.focus = Focus::Input(InputKind::MessageDraft);
    let fx = send(&mut app, Msg::Submit);
    assert!(sent_to(&fx).is_empty());
    assert_eq!(app.status.text, "Choose a conversation first");
}

#[test]
fn a_sent_message_is_read_back_with_its_status() {
    let mut app = inbox();
    send(&mut app, Msg::Select(0));
    send(
        &mut app,
        Msg::MessageSent {
            peer: "~dev".into(),
        },
    );
    send(
        &mut app,
        Msg::ThreadLoaded {
            peer: "~dev".into(),
            messages: vec![
                msg(4, "~dev", true, "on my way", Status::Delivered),
                msg(5, "~dev", false, "great", Status::Received),
            ],
        },
    );
    let view = view_model::messages_view(&app);
    assert_eq!(view.thread.len(), 2);
    assert_eq!(view.thread[0].status, "Delivered");
    assert_eq!(view.thread[1].status, "");
    assert!(view.thread[1].selected, "the newest message is selected");
}

#[test]
fn a_thread_for_another_ship_is_ignored() {
    let mut app = inbox();
    send(&mut app, Msg::Select(0));
    send(
        &mut app,
        Msg::ThreadLoaded {
            peer: "~tyr".into(),
            messages: vec![msg(9, "~tyr", false, "photos are up", Status::Received)],
        },
    );
    assert!(app.messages.thread.is_empty());
}

#[test]
fn new_asks_for_a_ship_and_opens_its_thread() {
    let mut app = inbox();
    // A plain letter starts the filter, it does not run New.
    press(&mut app, Key::Char('n'));
    assert_eq!(app.focus, Focus::Input(InputKind::Filter));
    press(&mut app, Key::Escape);
    send(&mut app, Msg::New);
    assert_eq!(app.focus, Focus::Input(InputKind::NewPeer));
    assert_eq!(app.input.text, "~");
    app.input.text = "dev".into();
    let fx = send(&mut app, Msg::Submit);
    assert!(fx.is_empty());
    assert_eq!(app.status.text, "Enter a ship name like ~zod");
    app.input.text = "~dev".into();
    let fx = send(&mut app, Msg::Submit);
    assert!(fx.contains(&Effect::LoadThread {
        peer: "~dev".into()
    }));
    assert_eq!(app.focus, Focus::Input(InputKind::MessageDraft));
}

#[test]
fn delete_asks_and_removes_the_message_once_the_ship_confirms() {
    let mut app = inbox();
    send(&mut app, Msg::Select(0));
    send(
        &mut app,
        Msg::ThreadLoaded {
            peer: "~dev".into(),
            messages: vec![msg(3, "~dev", false, "see you at six", Status::Received)],
        },
    );
    send(&mut app, Msg::Delete);
    assert_eq!(app.focus, Focus::Confirm);
    let fx = send(&mut app, Msg::ConfirmAnswer(true));
    assert!(fx.contains(&Effect::DeleteMessage { id: 3 }));
    let fx = send(&mut app, Msg::MessageDeleted { id: 3 });
    assert!(app.messages.thread.is_empty());
    assert!(fx.contains(&Effect::LoadConversations));
}

#[test]
fn a_message_still_sending_cannot_be_deleted_yet() {
    let mut app = inbox();
    send(&mut app, Msg::Select(0));
    send(&mut app, Msg::BeginDraft);
    type_str(&mut app, "draft");
    send(&mut app, Msg::Submit);
    send(&mut app, Msg::Delete);
    assert_eq!(app.focus, Focus::Input(InputKind::MessageDraft));
    assert_eq!(app.status.text, "That message is still sending");
}

#[test]
fn poll_rereads_the_list_and_the_open_thread_only_in_messages() {
    let mut app = inbox();
    send(&mut app, Msg::Select(0));
    let fx = send(&mut app, Msg::Poll);
    assert_eq!(
        fx,
        vec![
            Effect::LoadConversations,
            Effect::LoadThread {
                peer: "~dev".into()
            }
        ]
    );
    send(&mut app, Msg::Switch(Section::Notes));
    assert!(send(&mut app, Msg::Poll).is_empty());
}

#[test]
fn a_failed_load_is_reported_for_messages() {
    let mut app = inbox();
    send(
        &mut app,
        Msg::LoadFailed {
            section: Section::Messages,
            error: "network error".into(),
        },
    );
    assert_eq!(app.status.text, "Could not load Messages: network error");
}

// ----- Files: folders can be renamed and moved -----

fn folders_app() -> App {
    let mut app = App::new();
    send(&mut app, Msg::Switch(Section::Files));
    send(
        &mut app,
        Msg::FilesLoaded {
            files: vec![
                file(1, "/school/cs101", "notes.pdf", 1),
                file(2, "/school", "plan.txt", 1),
                file(3, "/photos", "cover.jpg", 1),
            ],
        },
    );
    send(
        &mut app,
        Msg::DirsLoaded {
            dirs: vec!["/empty".into()],
        },
    );
    app
}

fn select_row(app: &mut App, name: &str) {
    let i = locus_core::view_model::file_rows(app)
        .iter()
        .position(|r| r.name == name)
        .expect("row");
    send(app, Msg::Select(i));
}

/// Open the folder called `name` in the current folder.
fn enter(app: &mut App, name: &str) {
    select_row(app, name);
    send(app, Msg::Activate);
}

#[test]
fn renaming_a_folder_moves_it_within_its_parent() {
    let mut app = folders_app();
    enter(&mut app, "school");
    select_row(&mut app, "cs101");
    send(&mut app, Msg::Rename);
    assert_eq!(app.focus, Focus::Input(InputKind::RenameDir));
    app.input.text = "cs102".into();
    let fx = send(&mut app, Msg::Submit);
    assert_eq!(
        fx,
        vec![Effect::MoveDir {
            from: "/school/cs101".into(),
            to: "/school/cs102".into()
        }]
    );
}

#[test]
fn moving_a_folder_takes_a_full_path() {
    let mut app = folders_app();
    select_row(&mut app, "school");
    send(&mut app, Msg::Move);
    assert_eq!(app.focus, Focus::Input(InputKind::MoveDir));
    app.input.text = "/archive/school".into();
    let fx = send(&mut app, Msg::Submit);
    assert_eq!(
        fx,
        vec![Effect::MoveDir {
            from: "/school".into(),
            to: "/archive/school".into()
        }]
    );
}

#[test]
fn a_folder_cannot_move_into_itself_or_the_root() {
    let mut app = folders_app();
    select_row(&mut app, "school");
    send(&mut app, Msg::Move);
    app.input.text = "/school/inside".into();
    let fx = send(&mut app, Msg::Submit);
    assert!(fx.is_empty());
    assert_eq!(app.status.text, "A folder cannot move into itself");

    send(&mut app, Msg::Move);
    app.input.text = "/".into();
    let fx = send(&mut app, Msg::Submit);
    assert!(fx.is_empty());
    assert_eq!(app.status.text, "A folder cannot be the root");
}

#[test]
fn a_folder_that_moves_while_open_is_followed() {
    let mut app = folders_app();
    enter(&mut app, "school");
    assert_eq!(app.files.cwd, "/school");
    send(
        &mut app,
        Msg::DirMoved {
            from: "/school".into(),
            to: "/archive/school".into(),
        },
    );
    assert_eq!(app.files.cwd, "/archive/school");
}

#[test]
fn a_failed_send_puts_the_text_back_in_the_draft() {
    let mut app = inbox();
    send(&mut app, Msg::Select(0));
    send(&mut app, Msg::BeginDraft);
    type_str(&mut app, "on my way");
    send(&mut app, Msg::Submit);
    send(
        &mut app,
        Msg::SendFailed {
            peer: "~dev".into(),
            body: "on my way".into(),
            error: "server returned HTTP 500".into(),
        },
    );
    let view = view_model::messages_view(&app);
    assert_eq!(view.draft, "on my way");
    assert!(view.thread.iter().all(|m| m.status != "Sending"));
    assert!(app.status.text.contains("not sent"));
}

#[test]
fn new_messages_in_the_open_conversation_are_marked_read() {
    let mut app = inbox();
    send(&mut app, Msg::Select(0));
    let fx = send(
        &mut app,
        Msg::ConversationsLoaded {
            conversations: vec![conv(
                "~dev",
                1,
                msg(10, "~dev", false, "are you there?", Status::Received),
            )],
        },
    );
    assert!(fx
        .iter()
        .any(|e| matches!(e, Effect::MarkRead { peer } if peer == "~dev")));
    assert_eq!(locus_core::messages::unread_total(&app), 0);
}

#[test]
fn a_closed_conversation_keeps_its_unread_count() {
    let mut app = inbox();
    send(&mut app, Msg::Select(0));
    let fx = send(
        &mut app,
        Msg::ConversationsLoaded {
            conversations: vec![
                conv("~dev", 0, msg(3, "~dev", false, "see you at six", Status::Received)),
                conv("~tyr", 3, msg(11, "~tyr", false, "and more", Status::Received)),
            ],
        },
    );
    assert!(!fx.iter().any(|e| matches!(e, Effect::MarkRead { .. })));
    assert_eq!(locus_core::messages::unread_total(&app), 3);
}
