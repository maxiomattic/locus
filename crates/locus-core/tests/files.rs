mod common;

use common::*;
use locus_core::{Effect, FileEntry, FilePatch, Focus, Key, Msg, view_model};

fn files_app() -> locus_core::App {
    let mut app = locus_core::App::new();
    send(&mut app, Msg::Switch(locus_core::Section::Files));
    send(
        &mut app,
        Msg::FilesLoaded {
            files: vec![
                file(1, "/", "readme.txt", 2),
                file(2, "/Photos/2026", "beach.jpg", 5),
                file(3, "/Photos", "cover.png", 1),
                file(4, "/Docs", "tax.pdf", 1),
            ],
        },
    );
    send(
        &mut app,
        Msg::DirsLoaded {
            dirs: vec!["/Music".into()],
        },
    );
    app
}

fn names(app: &locus_core::App) -> Vec<String> {
    view_model::file_rows(app)
        .into_iter()
        .map(|r| r.name)
        .collect()
}

#[test]
fn root_lists_folders_first_then_files() {
    let app = files_app();
    assert_eq!(names(&app), ["Docs", "Music", "Photos", "readme.txt"]);
    let rows = view_model::file_rows(&app);
    assert!(rows[0].is_dir && rows[0].size_label.is_empty());
    assert_eq!(rows[3].size_label, "2.0 KB");
    assert!(rows[0].selected);
}

#[test]
fn enter_and_l_go_into_folders_and_h_goes_back_to_the_same_row() {
    let mut app = files_app();
    press(&mut app, Key::Char('j')); // Music
    press(&mut app, Key::Char('j')); // Photos
    press(&mut app, Key::Enter);
    assert_eq!(app.files.cwd, "/Photos");
    assert_eq!(names(&app), ["2026", "cover.png"]);

    press(&mut app, Key::Char('h'));
    assert_eq!(app.files.cwd, "/");
    let rows = view_model::file_rows(&app);
    let selected: Vec<_> = rows
        .iter()
        .filter(|r| r.selected)
        .map(|r| r.name.clone())
        .collect();
    assert_eq!(
        selected,
        ["Photos"],
        "going up keeps the folder you came from selected"
    );
}

#[test]
fn ancestors_of_file_directories_appear_as_folders() {
    let mut app = files_app();
    press(&mut app, Key::Char('j'));
    press(&mut app, Key::Char('j'));
    press(&mut app, Key::Char('l'));
    assert_eq!(app.files.cwd, "/Photos");
    press(&mut app, Key::Enter);
    // /Photos/2026 exists only because beach.jpg lives there.
    assert_eq!(app.files.cwd, "/Photos/2026");
    assert_eq!(names(&app), ["beach.jpg"]);
}

#[test]
fn breadcrumb_follows_the_directory() {
    let mut app = files_app();
    app.files.cwd = "/Photos/2026".into();
    let crumbs: Vec<_> = view_model::breadcrumb(&app)
        .into_iter()
        .map(|c| (c.label, c.path))
        .collect();
    assert_eq!(
        crumbs,
        [
            ("/".to_string(), "/".to_string()),
            ("Photos".to_string(), "/Photos".to_string()),
            ("2026".to_string(), "/Photos/2026".to_string()),
        ]
    );
}

#[test]
fn h_at_root_does_nothing_and_backspace_goes_up() {
    let mut app = files_app();
    assert!(press(&mut app, Key::Char('h')).is_empty());
    assert_eq!(app.files.cwd, "/");
    app.files.cwd = "/Docs".into();
    press(&mut app, Key::Backspace);
    assert_eq!(app.files.cwd, "/");
}

#[test]
fn enter_on_a_file_opens_its_blob() {
    let mut app = files_app();
    // Move to readme.txt (last row).
    press(&mut app, Key::Char('G'));
    let fx = press(&mut app, Key::Enter);
    assert_eq!(
        fx,
        vec![Effect::OpenBlob {
            sha256: format!("{:064x}", 1),
            name: "readme.txt".into(),
            mime: "text/plain".into(),
        }]
    );
}

#[test]
fn y_copies_the_full_path() {
    let mut app = files_app();
    press(&mut app, Key::Char('G'));
    let fx = press(&mut app, Key::Char('y'));
    assert_eq!(fx, vec![Effect::CopyToClipboard("/readme.txt".into())]);
}

#[test]
fn new_folder_creates_under_the_current_directory() {
    let mut app = files_app();
    app.files.cwd = "/Photos".into();
    press(&mut app, Key::Char('n'));
    type_str(&mut app, "Trips");
    let fx = press(&mut app, Key::Enter);
    assert_eq!(
        fx,
        vec![Effect::CreateDir {
            path: "/Photos/Trips".into()
        }]
    );
    send(
        &mut app,
        Msg::DirCreated {
            path: "/Photos/Trips".into(),
        },
    );
    assert!(names(&app).contains(&"Trips".to_string()));
}

#[test]
fn new_folder_rejects_slashes() {
    let mut app = files_app();
    press(&mut app, Key::Char('n'));
    type_str(&mut app, "a/b");
    assert!(press(&mut app, Key::Enter).is_empty());
    assert_eq!(app.status.level, locus_core::Level::Error);
}

#[test]
fn rename_patches_with_the_file_revision() {
    let mut app = files_app();
    press(&mut app, Key::Char('G'));
    press(&mut app, Key::Char('r'));
    assert_eq!(app.input.text, "readme.txt");
    for _ in 0..app.input.text.len() {
        press(&mut app, Key::Backspace);
    }
    type_str(&mut app, "notes.txt");
    let fx = press(&mut app, Key::Enter);
    assert_eq!(
        fx,
        vec![Effect::PatchFile {
            id: 1,
            expected_revision: 2,
            patch: FilePatch {
                name: Some("notes.txt".into()),
                dir: None
            },
        }]
    );
}

#[test]
fn move_prefills_current_dir_and_patches_dir() {
    let mut app = files_app();
    press(&mut app, Key::Char('G'));
    press(&mut app, Key::Char('m'));
    assert_eq!(app.focus, Focus::Input(locus_core::InputKind::MoveFile));
    assert_eq!(app.input.text, "/");
    for _ in 0..app.input.text.len() {
        press(&mut app, Key::Backspace);
    }
    type_str(&mut app, "/Docs/");
    let fx = press(&mut app, Key::Enter);
    assert_eq!(
        fx,
        vec![Effect::PatchFile {
            id: 1,
            expected_revision: 2,
            patch: FilePatch {
                name: None,
                dir: Some("/Docs".into())
            },
        }]
    );
}

#[test]
fn move_rejects_relative_and_dotdot_paths() {
    let mut app = files_app();
    press(&mut app, Key::Char('G'));
    press(&mut app, Key::Char('m'));
    for _ in 0..app.input.text.len() {
        press(&mut app, Key::Backspace);
    }
    type_str(&mut app, "Docs/../etc");
    assert!(press(&mut app, Key::Enter).is_empty());
    assert_eq!(app.status.level, locus_core::Level::Error);
}

#[test]
fn delete_file_confirms_and_folders_cannot_be_deleted() {
    let mut app = files_app();
    press(&mut app, Key::Char('d'));
    assert_eq!(app.focus, Focus::List, "folder rows are not deletable");
    assert_eq!(app.status.text, "Folders cannot be deleted here");

    press(&mut app, Key::Char('G'));
    press(&mut app, Key::Char('d'));
    let fx = press(&mut app, Key::Char('y'));
    assert_eq!(fx, vec![Effect::DeleteFile { id: 1 }]);
}

#[test]
fn file_patch_updates_the_entry_in_place() {
    let mut app = files_app();
    let moved = FileEntry {
        dir: "/Docs".into(),
        revision: 3,
        ..file(1, "/", "readme.txt", 2)
    };
    send(&mut app, Msg::FilePatched { file: moved });
    assert!(
        app.files
            .files
            .iter()
            .any(|f| f.id == 1 && f.dir == "/Docs" && f.revision == 3)
    );
    assert_eq!(app.files.files.len(), 4);
}

#[test]
fn palette_goto_file_enters_its_directory_and_selects_it() {
    let mut app = files_app();
    send(
        &mut app,
        Msg::Goto {
            section: locus_core::Section::Files,
            id: 2,
        },
    );
    assert_eq!(app.files.cwd, "/Photos/2026");
    assert_eq!(names(&app), ["beach.jpg"]);
    assert!(view_model::file_rows(&app)[0].selected);
}

#[test]
fn upload_uses_the_current_directory() {
    let mut app = files_app();
    app.files.cwd = "/Docs".into();
    let fx = press(&mut app, Key::Char('u'));
    assert_eq!(
        fx,
        vec![Effect::UploadFiles {
            dir: "/Docs".into()
        }]
    );
    let fx = send(
        &mut app,
        Msg::UploadDone {
            dir: "/Docs".into(),
            count: 2,
        },
    );
    assert_eq!(fx, vec![Effect::LoadFiles, Effect::LoadDirs]);
}

#[test]
fn files_filter_only_searches_the_current_directory() {
    let mut app = files_app();
    press(&mut app, Key::Char('/'));
    type_str(&mut app, "TAX");
    assert!(
        names(&app).is_empty(),
        "tax.pdf is in /Docs, not at the root"
    );
    press(&mut app, Key::Enter);
    // The filter hides folders that do not match, so /Docs cannot be entered
    // from the root while filtering on "tax".
    assert_eq!(names(&app), Vec::<String>::new());
    app.files.cwd = "/Docs".into();
    assert_eq!(names(&app), ["tax.pdf"]);
}
