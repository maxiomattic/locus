mod common;

use common::*;
use locus_core::{Effect, Focus, Key, Msg, Section, view_model};

fn photos_app(columns: usize) -> locus_core::App {
    let mut app = locus_core::App::new();
    send(&mut app, Msg::Switch(Section::Photos));
    send(
        &mut app,
        Msg::Layout {
            photo_columns: columns,
        },
    );
    send(
        &mut app,
        Msg::PhotosLoaded {
            photos: vec![
                photo(1, "Pier", "sha256:0a1b2c"),
                photo(2, "Dunes", "holidays/dunes.jpg"),
                photo(3, "Sunset", "sha256:ffee"),
                photo(4, "Bridge", "holidays/bridge.jpg"),
                photo(5, "Night", "sha256:bad!"),
            ],
        },
    );
    app
}

#[test]
fn grid_moves_by_one_and_by_a_row() {
    let mut app = photos_app(2);
    press(&mut app, Key::Right);
    assert_eq!(app.photos.sel, 1);
    press(&mut app, Key::Down);
    assert_eq!(app.photos.sel, 3);
    press(&mut app, Key::Down);
    assert_eq!(app.photos.sel, 4, "clamped at the last photo");
    press(&mut app, Key::Up);
    assert_eq!(app.photos.sel, 2);
    press(&mut app, Key::Left);
    assert_eq!(app.photos.sel, 1);
    press(&mut app, Key::Left);
    press(&mut app, Key::Left);
    assert_eq!(app.photos.sel, 0, "clamped at the first photo");
}

#[test]
fn grid_rows_follow_the_column_count() {
    let app = photos_app(2);
    let grid = view_model::photo_grid(&app, 2);
    assert_eq!(grid.rows.len(), 3);
    assert_eq!(grid.rows[2].len(), 1);
    assert!(grid.rows[0][0].selected);
}

#[test]
fn sha256_paths_expose_the_hash_and_others_expose_the_path() {
    let app = photos_app(3);
    let grid = view_model::photo_grid(&app, 3);
    let cells: Vec<_> = grid.rows.iter().flatten().collect();
    assert_eq!(cells[0].hash.as_deref(), Some("0a1b2c"));
    assert_eq!(cells[1].hash, None);
    assert_eq!(cells[1].path, "holidays/dunes.jpg");
    assert_eq!(
        cells[4].hash, None,
        "non-hex after the prefix is not a hash"
    );
    assert_eq!(cells[4].path, "sha256:bad!");
}

#[test]
fn enter_opens_viewer_where_h_and_l_step_without_wrapping() {
    let mut app = photos_app(3);
    press(&mut app, Key::Enter);
    assert_eq!(app.focus, Focus::Detail);
    press(&mut app, Key::Left);
    assert_eq!(app.photos.sel, 0);
    press(&mut app, Key::Right);
    press(&mut app, Key::Right);
    assert_eq!(app.photos.sel, 2);
    let detail = view_model::photo_detail(&app).unwrap();
    assert_eq!(detail.title, "Sunset");
    assert_eq!(detail.position, "3 / 5");
    assert_eq!(detail.hash.as_deref(), Some("ffee"));
    press(&mut app, Key::Escape);
    assert_eq!(app.focus, Focus::List);
}

#[test]
fn u_uploads_into_photos() {
    let mut app = photos_app(3);
    let fx = ctrl(&mut app, 'o');
    assert_eq!(
        fx,
        vec![Effect::UploadFiles {
            dir: "/Photos".into()
        }]
    );
}

#[test]
fn photo_filter_matches_title_or_path() {
    let mut app = photos_app(3);
    app.filter = "holidays".into();
    assert_eq!(
        view_model::photo_grid(&app, 3)
            .rows
            .iter()
            .flatten()
            .count(),
        2
    );
}

#[test]
fn empty_photo_viewer_does_not_open() {
    let mut app = locus_core::App::new();
    send(&mut app, Msg::Switch(Section::Photos));
    send(&mut app, Msg::PhotosLoaded { photos: vec![] });
    press(&mut app, Key::Enter);
    assert_eq!(app.focus, Focus::List);
}

fn contacts_app() -> locus_core::App {
    let mut app = locus_core::App::new();
    send(&mut app, Msg::Switch(Section::Contacts));
    send(
        &mut app,
        Msg::ContactsLoaded {
            contacts: vec![
                contact(2, "Mara", "mara@example.org", "~mara"),
                contact(1, "ada", "", "~ada"),
            ],
        },
    );
    app
}

#[test]
fn contacts_are_sorted_by_name_ignoring_case() {
    let app = contacts_app();
    let names: Vec<_> = view_model::contact_rows(&app)
        .into_iter()
        .map(|r| r.name)
        .collect();
    assert_eq!(names, ["ada", "Mara"]);
    let row = &view_model::contact_rows(&app)[0];
    assert_eq!(row.subtitle, "~ada");
}

#[test]
fn new_contact_requires_a_name_and_an_at_sign_in_email() {
    let mut app = contacts_app();
    ctrl(&mut app, 'n');
    assert_eq!(app.focus, Focus::Form);
    let fx = send(&mut app, Msg::SaveForm);
    assert!(fx.is_empty());
    assert_eq!(
        view_model::form(&app).unwrap().error.as_deref(),
        Some("Name is required")
    );

    type_str(&mut app, "Grace");
    press(&mut app, Key::Tab);
    type_str(&mut app, "grace-at-example");
    let fx = send(&mut app, Msg::SaveForm);
    assert!(fx.is_empty());
    assert_eq!(
        view_model::form(&app).unwrap().error.as_deref(),
        Some("Email must contain @")
    );

    // Clear the email and save.
    for _ in 0.."grace-at-example".len() {
        press(&mut app, Key::Backspace);
    }
    let fx = send(&mut app, Msg::SaveForm);
    assert_eq!(
        fx,
        vec![Effect::CreateContact {
            name: "Grace".into(),
            email: String::new(),
            ship: String::new(),
        }]
    );
    send(
        &mut app,
        Msg::ContactSaved {
            contact: contact(9, "Grace", "", ""),
        },
    );
    assert_eq!(app.focus, Focus::List);
    assert!(app.form.is_none());
}

#[test]
fn edit_contact_patches_with_its_revision() {
    let mut app = contacts_app();
    press(&mut app, Key::F(2));
    let fx = send(&mut app, Msg::SaveForm);
    assert_eq!(
        fx,
        vec![Effect::PatchContact {
            id: 1,
            expected_revision: 1,
            patch: locus_core::locus_client::contacts::ContactPatch {
                name: Some("ada".into()),
                email: Some(String::new()),
                ship: Some("~ada".into()),
            },
        }]
    );
}

#[test]
fn delete_contact_confirms_then_deletes_the_selected_one() {
    let mut app = contacts_app();
    // Sorted order: ada (id 1) first.
    press(&mut app, Key::Delete);
    assert_eq!(app.focus, Focus::Confirm);
    assert_eq!(
        press(&mut app, Key::Char('y')),
        vec![Effect::DeleteContact { id: 1 }]
    );
    // A stray y afterwards is just a letter in the list, not a command.
    assert!(
        press(&mut app, Key::Char('y'))
            .iter()
            .all(|e| !matches!(e, Effect::DeleteContact { .. }))
    );
}

#[test]
fn enter_opens_contact_detail() {
    let mut app = contacts_app();
    press(&mut app, Key::Down);
    press(&mut app, Key::Enter);
    assert_eq!(app.focus, Focus::Detail);
    let d = view_model::contact_detail(&app).unwrap();
    assert_eq!(d.name, "Mara");
    assert_eq!(d.email, "mara@example.org");
}

#[test]
fn tab_in_contact_form_moves_fields() {
    let mut app = contacts_app();
    ctrl(&mut app, 'n');
    press(&mut app, Key::Tab);
    press(&mut app, Key::Tab);
    assert!(view_model::form(&app).unwrap().fields[2].active);
    press(&mut app, Key::Tab);
    assert!(
        view_model::form(&app).unwrap().fields[0].active,
        "wraps around"
    );
}

#[test]
fn delete_asks_then_removes_the_selected_photo() {
    let mut app = photos_app(3);
    press(&mut app, Key::Right);
    press(&mut app, Key::Delete);
    assert_eq!(
        view_model::confirm_prompt(&app).as_deref(),
        Some("Delete photo \"Dunes\"?")
    );
    assert_eq!(press(&mut app, Key::Enter), vec![Effect::DeletePhoto { id: 2 }]);
    send(&mut app, Msg::PhotoDeleted { id: 2 });
    assert_eq!(app.photos.items.len(), 4);
    assert!(app.photos.items.iter().all(|p| p.id != 2));
}
