//! `locus` — clan command plane (same functions the ship audits).

use locus_command::{
    execute_allowed, load_saved, run_approved_job, Session,
};
use std::env;
use std::io::{self, IsTerminal, Read};
use std::process::ExitCode;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args().skip(1);
    let cmd = args.next().unwrap_or_else(|| "help".into());
    match cmd.as_str() {
        "help" | "-h" | "--help" => {
            print_help();
            Ok(())
        }
        "login" => cmd_login(args.next()),
        "whoami" => {
            let s = Session::connect_saved().map_err(|e| e.to_string())?;
            println!("{}", serde_json::to_string_pretty(&s.whoami().map_err(|e| e.to_string())?).unwrap_or_default());
            Ok(())
        }
        "members" => {
            let s = Session::connect_saved().map_err(|e| e.to_string())?;
            println!("{}", serde_json::to_string_pretty(&s.members().map_err(|e| e.to_string())?).unwrap_or_default());
            Ok(())
        }
        "add" => {
            let ship = args.next().ok_or("usage: locus add <ship> <name> <role>")?;
            let name = args.next().ok_or("usage: locus add <ship> <name> <role>")?;
            let role = args.next().ok_or("usage: locus add <ship> <name> <role>")?;
            let s = Session::connect_saved().map_err(|e| e.to_string())?;
            let env = s.add_member(&ship, &name, &role).map_err(|e| e.to_string())?;
            println!("{}", serde_json::to_string_pretty(&env).unwrap_or_default());
            Ok(())
        }
        "issue-key" => {
            let ship = args.next().ok_or("usage: locus issue-key <ship>")?;
            let s = Session::connect_saved().map_err(|e| e.to_string())?;
            let env = s.issue_key(&ship).map_err(|e| e.to_string())?;
            println!("{}", serde_json::to_string_pretty(&env).unwrap_or_default());
            Ok(())
        }
        "grant" => {
            let ship = args.next().ok_or("usage: locus grant <ship> <role>")?;
            let role = args.next().ok_or("usage: locus grant <ship> <role>")?;
            let s = Session::connect_saved().map_err(|e| e.to_string())?;
            s.grant(&ship, &role).map_err(|e| e.to_string())?;
            Ok(())
        }
        "call" => {
            let fn_name = args.next().ok_or("usage: locus call <fn> [--as ~ship] [json-args]")?;
            let (as_ship, rest) = take_as(args.collect())?;
            let json_args = if rest.is_empty() {
                serde_json::json!({})
            } else {
                serde_json::from_str(&rest.join(" ")).map_err(|e| e.to_string())?
            };
            let s = Session::connect_saved().map_err(|e| e.to_string())?;
            let env = s.call(&fn_name, as_ship.as_deref(), json_args).map_err(|e| e.to_string())?;
            println!("{}", serde_json::to_string_pretty(&env).unwrap_or_default());
            Ok(())
        }
        "pending" => {
            let s = Session::connect_saved().map_err(|e| e.to_string())?;
            println!("{}", serde_json::to_string_pretty(&s.pending().map_err(|e| e.to_string())?).unwrap_or_default());
            Ok(())
        }
        "approve" => {
            let id = parse_id(args.next().ok_or("usage: locus approve <id>")?)?;
            let s = Session::connect_saved().map_err(|e| e.to_string())?;
            s.approve(id, None).map_err(|e| e.to_string())?;
            println!("approved {id}");
            Ok(())
        }
        "deny" => {
            let id = parse_id(args.next().ok_or("usage: locus deny <id>")?)?;
            let s = Session::connect_saved().map_err(|e| e.to_string())?;
            s.deny(id, None).map_err(|e| e.to_string())?;
            println!("denied {id}");
            Ok(())
        }
        "execute" => {
            let id = parse_id(args.next().ok_or("usage: locus execute <id>")?)?;
            let s = Session::connect_saved().map_err(|e| e.to_string())?;
            cmd_execute(&s, id)
        }
        "audit" => {
            let s = Session::connect_saved().map_err(|e| e.to_string())?;
            println!("{}", serde_json::to_string_pretty(&s.audit().map_err(|e| e.to_string())?).unwrap_or_default());
            Ok(())
        }
        other => Err(format!("unknown command {other}. try: locus help")),
    }
}

fn cmd_execute(s: &Session, id: u64) -> Result<(), String> {
    let jobs = s.jobs().map_err(|e| e.to_string())?;
    let job = jobs
        .into_iter()
        .find(|j| j.id == id)
        .ok_or_else(|| format!("no job {id}"))?;
    let execute = execute_allowed();
    let out = run_approved_job(&job, execute).map_err(|e| e.to_string())?;
    s.mark_executed(id).map_err(|e| e.to_string())?;
    if !out.trim().is_empty() {
        print!("{out}");
    }
    if execute {
        println!("executed {id}");
    } else {
        println!("dry-activate {id} (set LOCUS_OS_EXECUTE=1 to switch)");
    }
    Ok(())
}

fn print_help() {
    eprintln!(
        "locus — clan command plane\n\
         \n\
         locus login [code-file]   # +code file, or stdin; stores API key\n\
         locus whoami\n\
         locus members\n\
         locus add <ship> <name> <role>   # prints apiKey once\n\
         locus issue-key <ship>           # rotate a member key\n\
         locus grant <ship> <role>\n\
         locus call <fn> [--as ~ship] [json]\n\
         locus pending\n\
         locus approve <id>\n\
         locus deny <id>\n\
         locus execute <id>        # dry-activate unless LOCUS_OS_EXECUTE=1\n\
         locus audit\n\
         \n\
         Agents authenticate with their own X-Api-Key (from add / issue-key).\n\
         They can queue os.rebuild / os.rollback. They cannot\n\
         approve, execute, or call history.purge / zfs.destroy. --as is owner-only."
    );
}

fn cmd_login(code_file: Option<String>) -> Result<(), String> {
    let saved = load_saved();
    let code = if let Some(path) = code_file {
        std::fs::read_to_string(path).map_err(|e| e.to_string())?
    } else if !io::stdin().is_terminal() {
        let mut buf = String::new();
        io::stdin().read_to_string(&mut buf).map_err(|e| e.to_string())?;
        buf
    } else {
        return Err("usage: locus login <code-file>  (or pipe +code on stdin)".into());
    };
    Session::login(&saved.eyre_url, code.trim()).map_err(|e| e.to_string())?;
    eprintln!("logged in to {}", saved.eyre_url);
    Ok(())
}

fn take_as(args: Vec<String>) -> Result<(Option<String>, Vec<String>), String> {
    let mut as_ship = None;
    let mut rest = Vec::new();
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--as" {
            as_ship = Some(args.get(i + 1).cloned().ok_or("--as needs a ship")?);
            i += 2;
        } else {
            rest.push(args[i].clone());
            i += 1;
        }
    }
    Ok((as_ship, rest))
}

fn parse_id(s: String) -> Result<u64, String> {
    s.parse().map_err(|_| format!("not a job id: {s}"))
}
