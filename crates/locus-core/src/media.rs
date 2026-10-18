//! Movies and Music: two lists over the Files catalog, and the transport of
//! the one player. A movie is a video file in `/Movies`; music is an audio
//! file in `/Music`, grouped as `/Music/<artist>/<album>/<track>`.
//!
//! The core decides what plays next and keeps the list selections. It never
//! decodes anything: the shell draws the picture, plays the sound and reports
//! the position back as [`Msg::PlayerTick`].

use crate::msg::{Effect, FileEntry, Msg, Nav, PlayerCmd};
use crate::state::{App, Focus, Playback, Section, clamp_sel, matches_filter};
use crate::update::step;

/// Where new movies are uploaded and where the Movies list starts.
pub const MOVIES_DIR: &str = "/Movies";
/// Where new music is uploaded; albums live below it.
pub const MUSIC_DIR: &str = "/Music";

/// Past this position, Previous restarts the track instead of going back.
const RESTART_AFTER_MS: u64 = 3_000;

/// What a file is, from its mime type or, failing that, its extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Video,
    Audio,
    Image,
    Other,
}

const VIDEO_EXT: &[&str] = &[
    "mp4", "m4v", "mov", "mkv", "webm", "avi", "mpg", "mpeg", "ogv",
];
const AUDIO_EXT: &[&str] = &["mp3", "flac", "m4a", "aac", "ogg", "oga", "opus", "wav"];
const IMAGE_EXT: &[&str] = &["jpg", "jpeg", "png", "gif", "webp", "heic", "bmp"];

/// The lowercase extension of `name`, or `""`.
pub fn extension(name: &str) -> String {
    match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => ext.to_ascii_lowercase(),
        _ => String::new(),
    }
}

pub fn kind_of(name: &str, mime: &str) -> Kind {
    let mime = mime.to_ascii_lowercase();
    if mime.starts_with("video/") {
        return Kind::Video;
    }
    if mime.starts_with("audio/") {
        return Kind::Audio;
    }
    if mime.starts_with("image/") {
        return Kind::Image;
    }
    let ext = extension(name);
    if VIDEO_EXT.contains(&ext.as_str()) {
        Kind::Video
    } else if AUDIO_EXT.contains(&ext.as_str()) {
        Kind::Audio
    } else if IMAGE_EXT.contains(&ext.as_str()) {
        Kind::Image
    } else {
        Kind::Other
    }
}

/// A mime type for a file name, for playback when the catalog has none.
pub fn mime_for(name: &str) -> &'static str {
    match extension(name).as_str() {
        "mp4" | "m4v" => "video/mp4",
        "webm" => "video/webm",
        "mov" => "video/quicktime",
        "mkv" => "video/x-matroska",
        "ogv" => "video/ogg",
        "mpg" | "mpeg" => "video/mpeg",
        "avi" => "video/x-msvideo",
        "mp3" => "audio/mpeg",
        "flac" => "audio/flac",
        "m4a" | "aac" => "audio/mp4",
        "ogg" | "oga" | "opus" => "audio/ogg",
        "wav" => "audio/wav",
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        _ => "application/octet-stream",
    }
}

/// The mime type a file plays as: its catalog type when that is audio or
/// video, else one from its name.
pub fn playback_mime(file: &FileEntry) -> String {
    let m = file.mime.to_ascii_lowercase();
    if m.starts_with("video/") || m.starts_with("audio/") {
        m
    } else {
        mime_for(&file.name).to_string()
    }
}

/// A movie's title: the file name without its extension.
pub fn movie_title(name: &str) -> String {
    stem(name).to_string()
}

/// A track's title: the file name without its extension or leading track number.
pub fn track_title(name: &str) -> String {
    let base = stem(name);
    let digits = base.len() - base.trim_start_matches(|c: char| c.is_ascii_digit()).len();
    let rest =
        base[digits..].trim_start_matches([' ', '-', '.', '_', ')']);
    if digits == 0 || rest.is_empty() {
        base.to_string()
    } else {
        rest.to_string()
    }
}

fn stem(name: &str) -> &str {
    match name.rsplit_once('.') {
        Some((s, _)) if !s.is_empty() => s,
        _ => name,
    }
}

/// `(artist, album)` for a music folder. Below `/Music` the first folder is
/// the artist and the second is the album; anything else is unsorted.
pub fn artist_album(dir: &str) -> (String, String) {
    let rest = match dir.strip_prefix(MUSIC_DIR) {
        Some(r) => r.trim_matches('/'),
        None => return (String::new(), String::new()),
    };
    let mut parts = rest.split('/').filter(|p| !p.is_empty());
    let artist = parts.next().unwrap_or_default().to_string();
    let album = parts.collect::<Vec<_>>().join("/");
    (artist, album)
}

/// Whether `dir` is `root` or a folder below it.
pub fn is_within(dir: &str, root: &str) -> bool {
    dir.strip_prefix(root)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
}

/// Movies in the filter, by name: the videos at or below `/Movies`.
pub fn movies(app: &App) -> Vec<&FileEntry> {
    let mut out: Vec<&FileEntry> = app
        .files
        .files
        .iter()
        .filter(|f| is_within(&f.dir, MOVIES_DIR))
        .filter(|f| kind_of(&f.name, &f.mime) == Kind::Video)
        .filter(|f| matches_filter(&app.filter, &[&f.name]))
        .collect();
    out.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then(a.id.cmp(&b.id))
    });
    out
}

/// Music in the filter, grouped by folder so an album's tracks sit together:
/// the audio files at or below `/Music`.
pub fn music(app: &App) -> Vec<&FileEntry> {
    let mut out: Vec<&FileEntry> = app
        .files
        .files
        .iter()
        .filter(|f| is_within(&f.dir, MUSIC_DIR))
        .filter(|f| kind_of(&f.name, &f.mime) == Kind::Audio)
        .filter(|f| matches_filter(&app.filter, &[&f.name, &f.dir]))
        .collect();
    out.sort_by(|a, b| {
        a.dir
            .to_lowercase()
            .cmp(&b.dir.to_lowercase())
            .then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then(a.id.cmp(&b.id))
    });
    out
}

/// The rows of a media section. Other sections have none.
pub fn rows(app: &App, section: Section) -> Vec<&FileEntry> {
    match section {
        Section::Movies => movies(app),
        Section::Music => music(app),
        _ => Vec::new(),
    }
}

fn sel_mut(app: &mut App, section: Section) -> &mut usize {
    match section {
        Section::Music => &mut app.media.music_sel,
        _ => &mut app.media.movies_sel,
    }
}

/// The selected row of a media section, clamped to the rows shown.
pub fn selected(app: &App, section: Section) -> Option<FileEntry> {
    let list = rows(app, section);
    let i = match section {
        Section::Music => app.media.music_sel,
        _ => app.media.movies_sel,
    };
    list.get(clamp_sel(i, list.len())).map(|f| (*f).clone())
}

pub fn nav(app: &mut App, section: Section, nav: Nav) {
    let len = rows(app, section).len();
    let sel = sel_mut(app, section);
    *sel = step(*sel, len, nav);
}

pub fn select(app: &mut App, section: Section, i: usize) {
    let len = rows(app, section).len();
    *sel_mut(app, section) = clamp_sel(i, len);
}

pub fn reset_sel(app: &mut App) {
    app.media.movies_sel = 0;
    app.media.music_sel = 0;
}

/// Start `file` in the player. `from` is the app it was started from.
pub fn play(app: &mut App, fx: &mut Vec<Effect>, file: FileEntry, from: Section) {
    let is_video = kind_of(&file.name, &file.mime) == Kind::Video;
    let volume = app.player.as_ref().map_or(80, |p| p.volume);
    let mime = playback_mime(&file);
    fx.push(Effect::Playback(PlayerCmd::Load {
        sha256: file.sha256.clone(),
        name: file.name.clone(),
        mime,
        video: is_video,
        autoplay: true,
    }));
    fx.push(Effect::Playback(PlayerCmd::Volume(volume)));
    app.player = Some(Playback {
        file,
        is_video,
        from,
        playing: true,
        position_ms: 0,
        duration_ms: 0,
        volume,
        error: None,
    });
}

/// Enter in a media list: play the selected row.
pub fn activate(app: &mut App, fx: &mut Vec<Effect>, section: Section) {
    match selected(app, section) {
        Some(file) => play(app, fx, file, section),
        None => app.info(match section {
            Section::Music => "No music here yet. Upload to add some.",
            _ => "No movies here yet. Upload to add some.",
        }),
    }
}

pub fn toggle(app: &mut App, fx: &mut Vec<Effect>) {
    let Some(p) = app.player.as_mut() else {
        app.info("Nothing is playing");
        return;
    };
    p.playing = !p.playing;
    fx.push(Effect::Playback(if p.playing {
        PlayerCmd::Resume
    } else {
        PlayerCmd::Pause
    }));
}

pub fn seek(app: &mut App, fx: &mut Vec<Effect>, ms: u64) {
    let Some(p) = app.player.as_mut() else {
        return;
    };
    let end = if p.duration_ms > 0 { p.duration_ms } else { ms };
    p.position_ms = ms.min(end);
    fx.push(Effect::Playback(PlayerCmd::Seek(p.position_ms)));
}

pub fn set_volume(app: &mut App, fx: &mut Vec<Effect>, volume: u8) {
    let volume = volume.min(100);
    if let Some(p) = app.player.as_mut() {
        p.volume = volume;
    }
    fx.push(Effect::Playback(PlayerCmd::Volume(volume)));
}

pub fn stop(app: &mut App, fx: &mut Vec<Effect>) {
    if app.player.take().is_some() {
        fx.push(Effect::Playback(PlayerCmd::Stop));
    }
    if app.focus == Focus::Detail && app.section == Section::Photos {
        app.focus = Focus::List;
    }
}

/// The list the player walks: the app it was started from, if that is a
/// media list.
fn queue_section(app: &App) -> Option<Section> {
    let from = app.player.as_ref()?.from;
    matches!(from, Section::Movies | Section::Music).then_some(from)
}

/// Index of the playing file in its app's list, and the list.
fn playing_index(app: &App) -> Option<(Section, usize)> {
    let section = queue_section(app)?;
    let id = app.player.as_ref()?.file.id;
    let i = rows(app, section).iter().position(|f| f.id == id)?;
    Some((section, i))
}

/// Play the next row of the list the player came from. At the end, stop.
pub fn next(app: &mut App, fx: &mut Vec<Effect>) {
    let Some((section, i)) = playing_index(app) else {
        app.info("Nothing to go to next");
        return;
    };
    let list = rows(app, section);
    match list.get(i + 1) {
        Some(file) => {
            let file = (*file).clone();
            *sel_mut(app, section) = i + 1;
            play(app, fx, file, section);
        }
        None => {
            app.info("End of the list");
            stop(app, fx);
        }
    }
}

/// Restart the track when it is well in; otherwise go to the previous row.
pub fn previous(app: &mut App, fx: &mut Vec<Effect>) {
    let position = app.player.as_ref().map_or(0, |p| p.position_ms);
    if position > RESTART_AFTER_MS {
        seek(app, fx, 0);
        return;
    }
    let Some((section, i)) = playing_index(app) else {
        seek(app, fx, 0);
        return;
    };
    if i == 0 {
        seek(app, fx, 0);
        return;
    }
    let list = rows(app, section);
    let file = list[i - 1].clone();
    *sel_mut(app, section) = i - 1;
    play(app, fx, file, section);
}

/// The shell reports where playback is. Only the current player is updated.
pub fn tick(app: &mut App, position_ms: u64, duration_ms: u64, playing: bool) {
    if let Some(p) = app.player.as_mut() {
        p.position_ms = position_ms;
        if duration_ms > 0 {
            p.duration_ms = duration_ms;
        }
        p.playing = playing;
    }
}

/// The file reached its end. Music moves on; a movie or photo stops at the end.
pub fn ended(app: &mut App, fx: &mut Vec<Effect>) {
    let Some(p) = app.player.as_mut() else {
        return;
    };
    if p.from == Section::Music {
        next(app, fx);
    } else {
        p.playing = false;
        p.position_ms = p.duration_ms;
    }
}

pub fn failed(app: &mut App, error: String) {
    if let Some(p) = app.player.as_mut() {
        p.playing = false;
        p.error = Some(error.clone());
    }
    app.error(format!("Playback failed: {error}"));
}

/// Start a video photo in the viewer. `file` is the photo's blob as a file.
pub fn play_photo(app: &mut App, fx: &mut Vec<Effect>, file: FileEntry) {
    play(app, fx, file, Section::Photos);
}

/// Messages a shell can send without the user touching the list.
pub fn is_player_msg(msg: &Msg) -> bool {
    matches!(
        msg,
        Msg::TogglePlay
            | Msg::NextTrack
            | Msg::PrevTrack
            | Msg::SeekTo(_)
            | Msg::SetVolume(_)
            | Msg::StopPlayback
            | Msg::PlayerTick { .. }
            | Msg::PlayerEnded
            | Msg::PlayerFailed(_)
    )
}
