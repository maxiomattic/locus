//! Contacts section: list, detail pane, and the contact form.

use locus_client::contacts::{Contact, ContactPatch};

use crate::msg::{Effect, Msg};
use crate::state::{App, Focus, Form, FormField, FormKind, Section, clamp_sel, matches_filter};

/// Contacts in the filter, sorted by name.
pub fn visible(app: &App) -> Vec<&Contact> {
    let mut v: Vec<&Contact> = app
        .contacts
        .items
        .iter()
        .filter(|c| matches_filter(&app.filter, &[&c.name, &c.email, &c.ship]))
        .collect();
    v.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then(a.id.cmp(&b.id))
    });
    v
}

fn selected(app: &App) -> Option<&Contact> {
    visible(app).get(app.contacts.sel).copied()
}

pub fn loaded(app: &mut App, contacts: Vec<Contact>) {
    app.contacts.items = contacts;
    app.loading[Section::Contacts.index()] = false;
    app.loaded[Section::Contacts.index()] = true;
    app.contacts.sel = clamp_sel(app.contacts.sel, visible(app).len());
}

pub fn select(app: &mut App, id: u64) {
    if let Some(pos) = visible(app).iter().position(|c| c.id == id) {
        app.contacts.sel = pos;
    }
    app.focus = Focus::List;
}

pub fn open_detail(app: &mut App) {
    if selected(app).is_some() {
        app.focus = Focus::Detail;
    } else {
        app.info("No contact selected");
    }
}

fn field(label: &'static str, value: impl Into<String>) -> FormField {
    FormField {
        label,
        value: value.into(),
    }
}

fn open_form(app: &mut App, kind: FormKind, fields: Vec<FormField>) {
    app.form = Some(Form {
        kind,
        fields,
        active: 0,
        error: None,
    });
    app.focus = Focus::Form;
}

pub fn new_contact(app: &mut App) {
    open_form(
        app,
        FormKind::NewContact,
        vec![field("name", ""), field("email", ""), field("ship", "")],
    );
}

pub fn edit_selected(app: &mut App) {
    let Some(c) = selected(app).cloned() else {
        app.info("No contact selected");
        return;
    };
    open_form(
        app,
        FormKind::EditContact {
            id: c.id,
            revision: c.revision,
        },
        vec![
            field("name", c.name),
            field("email", c.email),
            field("ship", c.ship),
        ],
    );
}

pub fn ask_delete(app: &mut App) {
    let Some((id, name)) = selected(app).map(|c| (c.id, c.name.clone())) else {
        app.info("No contact selected");
        return;
    };
    app.ask(
        format!("Delete contact \"{name}\"?"),
        Msg::ExecuteDelete {
            section: Section::Contacts,
            id,
        },
    );
}

pub fn execute_delete(app: &mut App, fx: &mut Vec<Effect>, id: u64) {
    fx.push(Effect::DeleteContact { id });
    app.info("Deleting contact");
}

/// Validate and send the form. The form stays open until the save lands.
pub fn submit_form(app: &mut App, fx: &mut Vec<Effect>) {
    let Some(form) = app.form.clone() else {
        return;
    };
    let name = form.value("name").trim().to_string();
    let email = form.value("email").trim().to_string();
    let ship = form.value("ship").trim().to_string();
    let problem = if name.is_empty() {
        Some("Name is required")
    } else if !email.is_empty() && !email.contains('@') {
        Some("Email must contain @")
    } else {
        None
    };
    if let Some(problem) = problem {
        if let Some(f) = app.form.as_mut() {
            f.error = Some(problem.into());
        }
        app.error(problem);
        return;
    }
    match form.kind {
        FormKind::NewContact => fx.push(Effect::CreateContact { name, email, ship }),
        FormKind::EditContact { id, revision } => fx.push(Effect::PatchContact {
            id,
            expected_revision: revision,
            patch: ContactPatch {
                name: Some(name),
                email: Some(email),
                ship: Some(ship),
            },
        }),
        FormKind::NewEvent | FormKind::EditEvent { .. } => return,
    }
    if let Some(f) = app.form.as_mut() {
        f.error = None;
    }
    app.info("Saving contact");
}

pub fn saved(app: &mut App, contact: Contact) {
    let id = contact.id;
    match app.contacts.items.iter_mut().find(|c| c.id == id) {
        Some(slot) => *slot = contact,
        None => app.contacts.items.push(contact),
    }
    if matches!(
        app.form.as_ref().map(|f| &f.kind),
        Some(FormKind::NewContact | FormKind::EditContact { .. })
    ) {
        app.form = None;
        app.focus = Focus::List;
    }
    app.contacts.sel = visible(app).iter().position(|c| c.id == id).unwrap_or(0);
    app.info("Saved contact");
}

pub fn deleted(app: &mut App, id: u64) {
    app.contacts.items.retain(|c| c.id != id);
    if app.focus == Focus::Detail && selected(app).is_none() {
        app.focus = Focus::List;
    }
    app.contacts.sel = clamp_sel(app.contacts.sel, visible(app).len());
    app.info("Contact deleted");
}
