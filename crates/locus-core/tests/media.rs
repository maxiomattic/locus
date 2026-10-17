//! Movies and Music: which files they list, how playback is driven, and what
//! plays next. The shell owns the picture and sound; these tests check the
//! commands the core sends and the state it keeps.

mod common;

use common::*;
use locus_core::media::{self, Kind};
use locus_core::{Effect, FileEntry, Focus, PlayerCmd, Section, view_model};

fn media_file(id: u64, dir: &str, name: &str, mime: &str) -> FileEntry {
    FileEntry {
        mime: mime.into(),
        ..file(id, dir, name, 1)
    }
}

/// A library with two movies, two albums of music and one text file.
fn library() -> App {
    let mut app = App::new();
    send(&mut app, Msg::Switch(Section::Movies));
    send(
        &mut app,
        Msg::FilesLoaded {
            files: vec![
                media_file(1, "/Movies", "Heat (1995).mp4", "video/mp4"),
                media_file(2, "/Movies", "Alien.mkv", "application/octet-stream"),
                media_file(
                    3,
                    "/Music/Bach/Goldberg",
                    "02 - Variatio 1.flac",
                    "audio/flac",
                ),
                media_file(4, "/Music/Bach/Goldberg", "01 - Aria.flac", "audio/flac"),
                media_file(5, "/Music/Miles/Kind of Blue", "So What.mp3", "audio/mpeg"),
                media_file(6, "/Docs", "notes.txt", "text/plain"),
            ],
        },
    );
    send(&mut app, Msg::DirsLoaded { dirs: vec![] });
    app
}

fn music_app() -> App {
    let mut app = library();
    send(&mut app, Msg::Switch(Section::Music));
    app
}

fn movie_names(app: &App) -> Vec<String> {
    media::movies(app).iter().map(|f| f.name.clone()).collect()
}

fn track_names(app: &App) -> Vec<String> {
    media::music(app).iter().map(|f| f.name.clone()).collect()
}

fn loads(fx: &[Effect]) -> Vec<&PlayerCmd> {
    fx.iter()
        .filter_map(|e| match e {
            Effect::Playback(cmd @ PlayerCmd::Load { .. }) => Some(cmd),
            _ => None,
        })
        .collect()
}

#[test]
fn kind_comes_from_mime_then_extension() {
    assert_eq!(media::kind_of("a.MKV", ""), Kind::Video);
    assert_eq!(media::kind_of("clip", "video/webm"), Kind::Video);
    assert_eq!(media::kind_of("x.bin", "audio/flac"), Kind::Audio);
    assert_eq!(
        media::kind_of("x.mp3", "application/octet-stream"),
        Kind::Audio
    );
    assert_eq!(media::kind_of("pic.JPG", ""), Kind::Image);
    assert_eq!(media::kind_of("notes.txt", "text/plain"), Kind::Other);
}

#[test]
fn titles_drop_extensions_and_track_numbers() {
    assert_eq!(media::movie_title("Heat (1995).mp4"), "Heat (1995)");
    assert_eq!(media::track_title("01 - Aria.flac"), "Aria");
    assert_eq!(media::track_title("Variatio 1.flac"), "Variatio 1");
    assert_eq!(media::track_title("7.opus"), "7");
}

#[test]
fn albums_come_from_the_folder_under_music() {
    assert_eq!(
        media::artist_album("/Music/Bach/Goldberg"),
        ("Bach".to_string(), "Goldberg".to_string())
    );
    assert_eq!(
        media::artist_album("/Music/Loose"),
        ("Loose".into(), String::new())
    );
    assert_eq!(media::artist_album("/Docs"), (String::new(), String::new()));
}

#[test]
fn movies_list_only_video_and_music_only_audio() {
    let app = library();
    assert_eq!(movie_names(&app), vec!["Alien.mkv", "Heat (1995).mp4"]);
    // Sorted by folder, then name, so an album's tracks sit together.
    assert_eq!(
        track_names(&app),
        vec!["01 - Aria.flac", "02 - Variatio 1.flac", "So What.mp3"]
    );
}

#[test]
fn music_rows_show_an_album_heading_once_per_album() {
    let app = music_app();
    let rows = view_model::media_rows(&app, Section::Music);
    let headings: Vec<Option<&str>> = rows.iter().map(|r| r.heading.as_deref()).collect();
    assert_eq!(
        headings,
        vec![Some("Bach · Goldberg"), None, Some("Miles · Kind of Blue")]
    );
    assert_eq!(rows[0].title, "Aria");
}

#[test]
fn enter_plays_the_selected_movie_as_video() {
    let mut app = library();
    let fx = send(&mut app, Msg::Activate);
    let loaded = loads(&fx);
    assert_eq!(loaded.len(), 1);
    assert!(matches!(
        loaded[0],
        PlayerCmd::Load { name, mime, video: true, autoplay: true, .. }
            if name == "Alien.mkv" && mime == "video/x-matroska"
    ));
    let player = view_model::player(&app).expect("playing");
    assert!(player.is_video);
    assert!(player.playing);
    assert_eq!(player.title, "Alien");
}

#[test]
fn space_pauses_and_resumes_the_player() {
    let mut app = music_app();
    send(&mut app, Msg::Activate);
    let paused = send(&mut app, Msg::TogglePlay);
    assert_eq!(paused, vec![Effect::Playback(PlayerCmd::Pause)]);
    assert!(!app.player.as_ref().unwrap().playing);
    let resumed = press(&mut app, Key::Char(' '));
    assert_eq!(resumed, vec![Effect::Playback(PlayerCmd::Resume)]);
    assert!(app.player.as_ref().unwrap().playing);
}

#[test]
fn space_without_a_player_says_so() {
    let mut app = music_app();
    let fx = press(&mut app, Key::Char(' '));
    assert!(fx.is_empty());
    assert_eq!(app.status.text, "Nothing is playing");
}

#[test]
fn music_moves_to_the_next_track_at_the_end_of_a_track() {
    let mut app = music_app();
    send(&mut app, Msg::Activate); // Aria
    let fx = send(&mut app, Msg::PlayerEnded);
    assert!(
        matches!(loads(&fx)[0], PlayerCmd::Load { name, .. } if name == "02 - Variatio 1.flac")
    );
    assert_eq!(app.player.as_ref().unwrap().file.id, 3);
    assert_eq!(app.media.music_sel, 1, "the list follows the player");
}

#[test]
fn next_at_the_end_of_the_list_stops() {
    let mut app = music_app();
    send(&mut app, Msg::Nav(locus_core::Nav::Bottom));
    send(&mut app, Msg::Activate); // So What, the last track
    let fx = send(&mut app, Msg::NextTrack);
    assert_eq!(fx, vec![Effect::Playback(PlayerCmd::Stop)]);
    assert!(app.player.is_none());
    assert_eq!(app.status.text, "End of the list");
}

#[test]
fn previous_restarts_a_track_that_is_well_in() {
    let mut app = music_app();
    send(&mut app, Msg::Nav(locus_core::Nav::Bottom));
    send(&mut app, Msg::Activate); // So What
    send(
        &mut app,
        Msg::PlayerTick {
            position_ms: 12_000,
            duration_ms: 200_000,
            playing: true,
        },
    );
    let fx = send(&mut app, Msg::PrevTrack);
    assert_eq!(fx, vec![Effect::Playback(PlayerCmd::Seek(0))]);
    assert_eq!(app.player.as_ref().unwrap().file.id, 5, "same track");
}

#[test]
fn previous_near_the_start_goes_to_the_row_before() {
    let mut app = music_app();
    send(&mut app, Msg::Nav(locus_core::Nav::Down));
    send(&mut app, Msg::Activate); // Variatio 1, row 1
    let fx = send(&mut app, Msg::PrevTrack);
    assert!(matches!(loads(&fx)[0], PlayerCmd::Load { name, .. } if name == "01 - Aria.flac"));
}

#[test]
fn seeking_stays_within_the_known_length() {
    let mut app = library();
    send(&mut app, Msg::Activate);
    send(
        &mut app,
        Msg::PlayerTick {
            position_ms: 0,
            duration_ms: 60_000,
            playing: true,
        },
    );
    let fx = send(&mut app, Msg::SeekTo(90_000));
    assert_eq!(fx, vec![Effect::Playback(PlayerCmd::Seek(60_000))]);
    assert_eq!(app.player.as_ref().unwrap().position_ms, 60_000);
}

#[test]
fn a_movie_stops_at_its_end_instead_of_moving_on() {
    let mut app = library();
    send(&mut app, Msg::Activate); // Alien.mkv, first movie
    send(&mut app, Msg::PlayerEnded);
    let player = app.player.as_ref().expect("the movie stays loaded");
    assert!(!player.playing);
    assert_eq!(player.file.id, 2);
}

#[test]
fn a_failed_file_is_reported_on_the_player() {
    let mut app = library();
    send(&mut app, Msg::Activate);
    send(&mut app, Msg::PlayerFailed("codec missing".into()));
    let player = view_model::player(&app).unwrap();
    assert_eq!(player.error.as_deref(), Some("codec missing"));
    assert!(!player.playing);
    assert_eq!(app.status.text, "Playback failed: codec missing");
}

#[test]
fn upload_in_movies_and_music_goes_to_their_folders() {
    let mut app = library();
    let fx = send(&mut app, Msg::Upload);
    assert_eq!(
        fx,
        vec![Effect::UploadFiles {
            dir: "/Movies".into()
        }]
    );
    send(&mut app, Msg::Switch(Section::Music));
    let fx = send(&mut app, Msg::Upload);
    assert_eq!(
        fx,
        vec![Effect::UploadFiles {
            dir: "/Music".into()
        }]
    );
}

#[test]
fn delete_in_movies_asks_then_deletes_the_file() {
    let mut app = library();
    send(&mut app, Msg::Delete);
    assert_eq!(app.focus, Focus::Confirm);
    let fx = send(&mut app, Msg::ConfirmAnswer(true));
    assert_eq!(fx, vec![Effect::DeleteFile { id: 2 }]);
}

#[test]
fn rename_in_music_edits_the_selected_track() {
    let mut app = music_app();
    send(&mut app, Msg::Rename);
    assert_eq!(app.focus, Focus::Input(locus_core::InputKind::RenameFile));
    assert_eq!(app.input.target, Some(4));
    assert_eq!(app.input.text, "01 - Aria.flac");
}

#[test]
fn a_video_photo_plays_in_the_viewer_and_an_image_does_not() {
    let mut app = App::new();
    send(&mut app, Msg::Switch(Section::Photos));
    send(
        &mut app,
        Msg::PhotosLoaded {
            photos: vec![
                photo(1, "holiday.jpg", &format!("sha256:{}", "a".repeat(64))),
                photo(2, "birthday.mp4", &format!("sha256:{}", "b".repeat(64))),
            ],
        },
    );
    let fx = send(&mut app, Msg::Activate);
    assert!(loads(&fx).is_empty(), "the first photo is an image");
    assert!(app.player.is_none());
    send(&mut app, Msg::Escape);
    send(&mut app, Msg::Nav(locus_core::Nav::Right));
    let fx = send(&mut app, Msg::Activate);
    let loaded = loads(&fx);
    assert!(matches!(loaded[0], PlayerCmd::Load { video: true, .. }));
    let detail = view_model::photo_detail(&app).unwrap();
    assert!(detail.is_video);
}
