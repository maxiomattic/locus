//! Every operation a person can do has to finish within one frame, 16.6 ms.
//!
//! This times the core side of each operation against a large library: the
//! `update` call, then the view models a frame draws for the section on
//! screen. Drawing and layout are the shell's, and the shells keep their lists
//! virtual so a frame only builds the rows on screen.
//!
//! The budget is asserted in release builds, where the numbers mean something
//! (`cargo test --release -p locus-core --test perf`). Debug builds print the
//! table and skip the assertion.

mod common;

use std::time::{Duration, Instant};

use common::*;
use locus_client::messages::{Conversation, Message, Status};
use locus_core::{Key, Mods, Nav, Section, view_model as vm};

const FRAME: Duration = Duration::from_micros(16_600);
const BASE: u64 = 1_792_200_000;

fn big_library() -> App {
    let mut app = App::new();
    send(&mut app, Msg::Switch(Section::Files));

    // Files: 10,000 entries over 400 folders, with movies and music among them.
    let mut files = Vec::with_capacity(10_000);
    for i in 0..10_000u64 {
        let f = match i % 10 {
            0 | 1 => file(i + 1, "/Movies", &format!("Movie {i:05} (2001).mp4"), 1),
            2..=4 => file(
                i + 1,
                &format!("/Music/Artist {:03}/Album {:02}", i % 97, i % 13),
                &format!("{:02} - Track {i:05}.flac", i % 20),
                1,
            ),
            _ => file(
                i + 1,
                &format!("/Docs/folder {:03}", i % 400),
                &format!("document {i:05}.pdf"),
                1,
            ),
        };
        files.push(f);
    }
    send(&mut app, Msg::FilesLoaded { files });
    let dirs: Vec<String> = (0..400).map(|i| format!("/Docs/folder {i:03}")).collect();
    send(&mut app, Msg::DirsLoaded { dirs });

    // Photos: 5,000, a fifth of them videos.
    let photos: Vec<_> = (0..5_000u64)
        .map(|i| {
            let title = if i % 5 == 0 {
                format!("clip {i:05}.mp4")
            } else {
                format!("photo {i:05}.jpg")
            };
            photo(i + 1, &title, &format!("sha256:{:064x}", i + 1))
        })
        .collect();
    send(&mut app, Msg::PhotosLoaded { photos });

    // Notes: 2,000 across the two notebooks, each with a few task lines.
    send(
        &mut app,
        Msg::NotebooksLoaded {
            notebooks: vec![nb(), nb_journal()],
        },
    );
    let folders: Vec<_> = (0..100)
        .map(|i| folder(i + 100, &format!("Folder {i:03}"), Some(10)))
        .collect();
    send(&mut app, Msg::FoldersLoaded { nb: nb(), folders });
    let body = "Some text about the day.\n- [ ] first task\n- [x] done task\n- [ ] another task\n";
    let notes: Vec<_> = (0..2_000u64)
        .map(|i| {
            note_in(
                i + 1,
                100 + i % 100,
                &format!("Note {i:05}"),
                body,
                BASE + i,
            )
        })
        .collect();
    send(&mut app, Msg::NotesLoaded { nb: nb(), notes });
    let journal: Vec<_> = (0..200u64)
        .map(|i| note_in(i + 1, 20, &format!("Entry {i:04}"), body, BASE + i))
        .collect();
    send(
        &mut app,
        Msg::NotesLoaded {
            nb: nb_journal(),
            notes: journal,
        },
    );

    // Contacts: 2,000.
    let contacts: Vec<_> = (0..2_000u64)
        .map(|i| {
            contact(
                i + 1,
                &format!("Person {i:05}"),
                &format!("p{i}@example.org"),
                &format!("~dev-{i:04}"),
            )
        })
        .collect();
    send(&mut app, Msg::ContactsLoaded { contacts });

    // Calendar: 1,500 events, hourly, around today.
    let events: Vec<_> = (0..1_500u64)
        .map(|i| {
            event(
                i + 1,
                &format!("Event {i:04}"),
                BASE + i * 3_600,
                BASE + i * 3_600 + 1_800,
                1,
            )
        })
        .collect();
    send(&mut app, Msg::EventsLoaded { events });

    // Messages: 50 conversations of 60 messages each.
    let conversations: Vec<Conversation> = (0..50)
        .map(|c| {
            let peer = format!("~peer-{c:02}");
            Conversation {
                peer: peer.clone(),
                count: 60,
                unread: c % 3,
                last: Some(message(
                    60 * c + 59,
                    &peer,
                    false,
                    "the last message in this conversation",
                )),
            }
        })
        .collect();
    send(&mut app, Msg::ConversationsLoaded { conversations });
    let thread: Vec<Message> = (0..200u64)
        .map(|i| {
            message(
                i + 1,
                "~peer-00",
                i % 2 == 0,
                &format!("message number {i} of the thread"),
            )
        })
        .collect();
    send(
        &mut app,
        Msg::ThreadLoaded {
            peer: "~peer-00".into(),
            messages: thread,
        },
    );

    send(&mut app, Msg::Switch(Section::Notes));
    app
}

fn message(id: u64, peer: &str, outgoing: bool, body: &str) -> Message {
    Message {
        id,
        peer: peer.into(),
        outgoing,
        body: body.into(),
        sent: "~2026.10.17..09.00.00..0000".into(),
        status: if outgoing {
            Status::Delivered
        } else {
            Status::Received
        },
    }
}

/// What one frame draws for the section on screen, as view models.
fn frame(app: &App) {
    std::hint::black_box(vm::status_line(app));
    std::hint::black_box(vm::input_prompt(app));
    std::hint::black_box(vm::player(app));
    match app.section {
        Section::Notes => {
            std::hint::black_box(vm::notes_list(app));
            std::hint::black_box(vm::editor(app));
        }
        Section::Todo => {
            std::hint::black_box(vm::todo_groups(app));
        }
        Section::Calendar => {
            std::hint::black_box(vm::agenda(app));
            std::hint::black_box(vm::month_grid(app));
        }
        Section::Photos => {
            std::hint::black_box(vm::photo_grid(app, app.photos.columns));
            std::hint::black_box(vm::photo_detail(app));
        }
        Section::Files => {
            std::hint::black_box(vm::file_rows(app));
            std::hint::black_box(vm::breadcrumb(app));
        }
        Section::Contacts => {
            std::hint::black_box(vm::contact_rows(app));
            std::hint::black_box(vm::contact_detail(app));
        }
        Section::Movies | Section::Music => {
            std::hint::black_box(vm::media_rows(app, app.section));
        }
        Section::Messages => {
            std::hint::black_box(vm::messages_view(app));
        }
    }
}

struct Timing {
    name: String,
    elapsed: Duration,
}

/// Run `op` against the app, then draw a frame; keep the time.
fn timed(
    timings: &mut Vec<Timing>,
    app: &mut App,
    name: &str,
    op: impl FnOnce(&mut App) -> Vec<Effect>,
) {
    let start = Instant::now();
    let fx = op(app);
    std::hint::black_box(&fx);
    frame(app);
    timings.push(Timing {
        name: name.to_string(),
        elapsed: start.elapsed(),
    });
}

fn press_key(app: &mut App, key: Key) -> Vec<Effect> {
    match keymap(app, key, Mods::NONE) {
        Some(msg) => update(app, msg, now()),
        None => Vec::new(),
    }
}

#[test]
fn every_operation_fits_one_frame() {
    let mut app = big_library();
    let mut t: Vec<Timing> = Vec::new();

    // Ingesting what the server sends.
    timed(&mut t, &mut app, "load 10,000 files", |a| {
        let files: Vec<_> = (0..10_000u64)
            .map(|i| file(i + 1, "/Docs", &format!("f{i}.txt"), 1))
            .collect();
        send(a, Msg::FilesLoaded { files })
    });
    timed(&mut t, &mut app, "load 5,000 photos", |a| {
        let photos: Vec<_> = (0..5_000u64)
            .map(|i| photo(i + 1, &format!("p{i}.jpg"), ""))
            .collect();
        send(a, Msg::PhotosLoaded { photos })
    });
    timed(&mut t, &mut app, "load 2,000 contacts", |a| {
        let contacts: Vec<_> = (0..2_000u64)
            .map(|i| contact(i + 1, &format!("c{i}"), "", ""))
            .collect();
        send(a, Msg::ContactsLoaded { contacts })
    });
    timed(&mut t, &mut app, "load 200-message thread", |a| {
        let messages: Vec<_> = (0..200u64)
            .map(|i| message(i + 1, "~peer-00", false, "hi"))
            .collect();
        send(
            a,
            Msg::ThreadLoaded {
                peer: "~peer-00".into(),
                messages,
            },
        )
    });

    // Every section: switch, then move around its list.
    for section in Section::ALL {
        let name = section.label();
        timed(&mut t, &mut app, &format!("switch to {name}"), |a| {
            send(a, Msg::Switch(section))
        });
        for (label, nav) in [
            ("down", Nav::Down),
            ("page down", Nav::HalfDown),
            ("end", Nav::Bottom),
            ("home", Nav::Top),
        ] {
            timed(&mut t, &mut app, &format!("{name}: {label}"), |a| {
                send(a, Msg::Nav(nav))
            });
        }
        timed(&mut t, &mut app, &format!("{name}: select row 4000"), |a| {
            send(a, Msg::Select(4_000))
        });
    }

    // Files: browse, filter, open.
    timed(&mut t, &mut app, "files: open a folder", |a| {
        send(a, Msg::Switch(Section::Files));
        send(a, Msg::Nav(Nav::Top));
        send(a, Msg::Activate)
    });
    timed(&mut t, &mut app, "files: parent folder", |a| {
        press_key(a, Key::Backspace)
    });
    timed(&mut t, &mut app, "files: type to filter", |a| {
        press_key(a, Key::Char('d'));
        press_key(a, Key::Char('o'))
    });
    timed(&mut t, &mut app, "files: clear filter", |a| {
        press_key(a, Key::Escape)
    });
    timed(&mut t, &mut app, "files: new folder prompt", |a| {
        send(a, Msg::New)
    });
    timed(&mut t, &mut app, "files: cancel prompt", |a| {
        press_key(a, Key::Escape)
    });

    // Media: play, pause, seek, move on.
    timed(&mut t, &mut app, "music: play a track", |a| {
        send(a, Msg::Switch(Section::Music));
        send(a, Msg::Select(0));
        send(a, Msg::Activate)
    });
    timed(&mut t, &mut app, "music: pause", |a| {
        send(a, Msg::TogglePlay)
    });
    timed(&mut t, &mut app, "music: resume", |a| {
        send(a, Msg::TogglePlay)
    });
    timed(&mut t, &mut app, "music: position tick", |a| {
        send(
            a,
            Msg::PlayerTick {
                position_ms: 61_000,
                duration_ms: 240_000,
                playing: true,
            },
        )
    });
    timed(&mut t, &mut app, "music: seek", |a| {
        send(a, Msg::SeekTo(120_000))
    });
    timed(&mut t, &mut app, "music: volume", |a| {
        send(a, Msg::SetVolume(40))
    });
    timed(&mut t, &mut app, "music: next track", |a| {
        send(a, Msg::NextTrack)
    });
    timed(&mut t, &mut app, "music: previous track", |a| {
        send(a, Msg::PrevTrack)
    });
    timed(&mut t, &mut app, "music: track ended", |a| {
        send(a, Msg::PlayerEnded)
    });
    timed(&mut t, &mut app, "movies: play", |a| {
        send(a, Msg::Switch(Section::Movies));
        send(a, Msg::Select(3));
        send(a, Msg::Activate)
    });
    timed(&mut t, &mut app, "movies: stop", |a| {
        send(a, Msg::StopPlayback)
    });
    timed(&mut t, &mut app, "music: filter by name", |a| {
        send(a, Msg::Switch(Section::Music));
        press_key(a, Key::Char('t'));
        press_key(a, Key::Char('r'));
        press_key(a, Key::Char('a'))
    });
    timed(&mut t, &mut app, "music: clear filter", |a| {
        press_key(a, Key::Escape)
    });

    // Photos: grid, viewer, video.
    timed(&mut t, &mut app, "photos: grid to the end", |a| {
        send(a, Msg::Switch(Section::Photos));
        send(a, Msg::Nav(Nav::Bottom))
    });
    timed(&mut t, &mut app, "photos: open the viewer", |a| {
        send(a, Msg::Activate)
    });
    timed(&mut t, &mut app, "photos: next photo", |a| {
        send(a, Msg::Nav(Nav::Right))
    });
    timed(&mut t, &mut app, "photos: back to the grid", |a| {
        press_key(a, Key::Escape)
    });

    // Notes: open, type, switch notebooks.
    timed(&mut t, &mut app, "notes: open a note", |a| {
        send(a, Msg::Switch(Section::Notes));
        send(a, Msg::Nav(Nav::Top));
        send(a, Msg::Activate)
    });
    timed(&mut t, &mut app, "notes: type one character", |a| {
        let body =
            "Some text about the day.\n- [ ] first task\n- [x] done task\n- [ ] another task\nz";
        send(a, Msg::EditorChanged(body.into()))
    });
    timed(&mut t, &mut app, "notes: type into a 10 KB note", |a| {
        let body = "word ".repeat(2_000);
        send(a, Msg::EditorChanged(body))
    });
    timed(&mut t, &mut app, "notes: back to the list", |a| {
        press_key(a, Key::Escape)
    });
    timed(&mut t, &mut app, "notes: filter", |a| {
        press_key(a, Key::Char('n'));
        press_key(a, Key::Char('o'));
        press_key(a, Key::Char('t'))
    });
    timed(&mut t, &mut app, "notes: clear filter", |a| {
        press_key(a, Key::Escape)
    });

    // Calendar and tasks.
    timed(&mut t, &mut app, "calendar: next month", |a| {
        send(a, Msg::Switch(Section::Calendar));
        send(a, Msg::ShiftMonth(1))
    });
    timed(&mut t, &mut app, "calendar: today", |a| send(a, Msg::Today));
    timed(&mut t, &mut app, "calendar: choose a day", |a| {
        send(a, Msg::SelectDay(date(2026, 10, 20)))
    });
    timed(&mut t, &mut app, "todo: move down", |a| {
        send(a, Msg::Switch(Section::Todo));
        send(a, Msg::Nav(Nav::Down))
    });

    // Messages: choose, read, draft, send.
    timed(&mut t, &mut app, "messages: open a conversation", |a| {
        send(a, Msg::Switch(Section::Messages));
        send(a, Msg::Select(7))
    });
    timed(&mut t, &mut app, "messages: next conversation", |a| {
        send(a, Msg::Nav(Nav::Down))
    });
    timed(&mut t, &mut app, "messages: start typing a reply", |a| {
        send(a, Msg::BeginDraft);
        press_key(a, Key::Char('h'));
        press_key(a, Key::Char('i'))
    });
    timed(&mut t, &mut app, "messages: send", |a| send(a, Msg::Submit));
    timed(&mut t, &mut app, "messages: poll", |a| send(a, Msg::Poll));
    timed(&mut t, &mut app, "messages: thread read back", |a| {
        let messages: Vec<_> = (0..200u64)
            .map(|i| message(i + 1, "~peer-00", i % 2 == 0, "hi"))
            .collect();
        send(
            a,
            Msg::ThreadLoaded {
                peer: "~peer-00".into(),
                messages,
            },
        )
    });
    timed(&mut t, &mut app, "contacts: choose one", |a| {
        send(a, Msg::Switch(Section::Contacts));
        send(a, Msg::Select(1_500))
    });

    report(&t);
    if cfg!(not(debug_assertions)) {
        let slow: Vec<&Timing> = t.iter().filter(|x| x.elapsed > FRAME).collect();
        assert!(
            slow.is_empty(),
            "operations over one frame (16.6 ms): {:?}",
            slow.iter()
                .map(|x| format!("{} {:?}", x.name, x.elapsed))
                .collect::<Vec<_>>()
        );
    }
}

fn report(t: &[Timing]) {
    let mut sorted: Vec<&Timing> = t.iter().collect();
    sorted.sort_by_key(|t| std::cmp::Reverse(t.elapsed));
    let max = sorted.first().map(|x| x.elapsed).unwrap_or_default();
    eprintln!("\n{} operations; slowest {:?}", t.len(), max);
    for x in sorted.iter().take(12) {
        eprintln!("  {:>10?}  {}", x.elapsed, x.name);
    }
}
