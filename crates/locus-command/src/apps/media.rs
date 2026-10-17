//! `locus movies` and `locus music`: the Movies and Music apps from the
//! command line. Both are views over `%files`: a movie is a video file in
//! `/Movies`, a track is an audio file in `/Music/<artist>/<album>/`. Playing
//! is the app's job (`locus open movies`); here you list, upload and tidy.

use locus_client::files::FileEntry;
use locus_core::media::{self, Kind};
use serde_json::json;

use super::Ctx;
use super::args::Parsed;
use super::files;
use super::fmt::{self, client_error, emit, one_line, stamp, table};

/// Help for `locus movies --help`.
pub const MOVIES_HELP: &str = "locus movies <command>

  ls [--json]                       list the movies (video files)
  put <local file> [<dir>]          upload a movie (default folder /Movies)
  get <path-or-id> [-o <path>]      download
  mv <path-or-id> <dest dir>        move to another folder
  rename <path-or-id> <new name>    rename
  rm <path-or-id>                   delete

Play with the app: locus open movies
";

/// Help for `locus music --help`.
pub const MUSIC_HELP: &str = "locus music <command>

  ls [--json]                       list the tracks (audio files), by album
  put <local file> [<dir>]          upload a track (default folder /Music)
  get <path-or-id> [-o <path>]      download
  mv <path-or-id> <dest dir>        move to another folder
  rename <path-or-id> <new name>    rename
  rm <path-or-id>                   delete

Albums are folders: /Music/<artist>/<album>/<track>. Play with: locus open music
";

/// Which library: movies or music.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Library {
    Movies,
    Music,
}

impl Library {
    fn wanted(self) -> Kind {
        match self {
            Library::Movies => Kind::Video,
            Library::Music => Kind::Audio,
        }
    }

    fn home(self) -> &'static str {
        match self {
            Library::Movies => media::MOVIES_DIR,
            Library::Music => media::MUSIC_DIR,
        }
    }

    fn help(self) -> &'static str {
        match self {
            Library::Movies => MOVIES_HELP,
            Library::Music => MUSIC_HELP,
        }
    }
}

/// Run one `locus movies|music` command.
pub async fn run(ctx: &Ctx, lib: Library, cmd: &str, args: Vec<String>) -> Result<(), String> {
    match cmd {
        "ls" => ls(ctx, lib, args).await,
        "put" => files::put_into(ctx, args, lib.home()).await,
        "get" | "mv" | "rename" | "rm" => files::run(ctx, cmd, args).await,
        other => Err(format!(
            "unknown command {other}. try: locus {} --help",
            name(lib)
        )),
    }
}

fn name(lib: Library) -> &'static str {
    match lib {
        Library::Movies => "movies",
        Library::Music => "music",
    }
}

/// The help text, for `locus movies --help` and bare `locus movies`.
pub fn help(lib: Library) -> &'static str {
    lib.help()
}

/// The files of one kind, in the order the app lists them: movies by name,
/// tracks by folder then name.
pub fn library(files: &[FileEntry], lib: Library) -> Vec<&FileEntry> {
    let mut out: Vec<&FileEntry> = files
        .iter()
        .filter(|f| media::kind_of(&f.name, &f.mime) == lib.wanted())
        .collect();
    match lib {
        Library::Movies => out.sort_by_key(|f| f.name.to_lowercase()),
        Library::Music => out.sort_by(|a, b| {
            a.dir
                .to_lowercase()
                .cmp(&b.dir.to_lowercase())
                .then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        }),
    }
    out
}

async fn ls(ctx: &Ctx, lib: Library, args: Vec<String>) -> Result<(), String> {
    let p = Parsed::parse(&args, &[], &["--json"])?;
    p.positionals(0, 0, "locus movies ls [--json]")?;
    let client = ctx.client().await?;
    let all = client.list_files(None).await.map_err(client_error)?;
    let items = library(&all, lib);
    if p.has("--json") {
        let rows: Vec<_> = items
            .iter()
            .map(|f| match lib {
                Library::Movies => json!({
                    "id": f.id,
                    "folder": f.dir,
                    "title": media::movie_title(&f.name),
                    "name": f.name,
                    "size": f.size,
                    "mime": f.mime,
                }),
                Library::Music => {
                    let (artist, album) = media::artist_album(&f.dir);
                    json!({
                        "id": f.id,
                        "artist": artist,
                        "album": album,
                        "title": media::track_title(&f.name),
                        "name": f.name,
                        "size": f.size,
                        "mime": f.mime,
                    })
                }
            })
            .collect();
        fmt::json(&json!({ "items": rows }));
        return Ok(());
    }
    let rows: Vec<Vec<String>> = items
        .iter()
        .map(|f| match lib {
            Library::Movies => vec![
                f.id.to_string(),
                locus_core::view_model::size_label(f.size),
                stamp(f.modified, ctx.tz),
                one_line(&media::movie_title(&f.name)),
            ],
            Library::Music => {
                let (artist, album) = media::artist_album(&f.dir);
                vec![
                    f.id.to_string(),
                    locus_core::view_model::size_label(f.size),
                    one_line(&artist),
                    one_line(&album),
                    one_line(&media::track_title(&f.name)),
                ]
            }
        })
        .collect();
    match lib {
        Library::Movies => emit(&table(&["ID", "SIZE", "MODIFIED", "TITLE"], &rows)),
        Library::Music => emit(&table(&["ID", "SIZE", "ARTIST", "ALBUM", "TRACK"], &rows)),
    }
    Ok(())
}
