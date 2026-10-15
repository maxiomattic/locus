//! Hand-rolled argument parsing shared by the app commands.
//!
//! Flags are declared per command: `valued` flags take one value (`--flag v`
//! or `--flag=v`), `switches` take none. Everything else starting with `-` is
//! an error; a lone `-` and anything after `--` are positional.

use std::collections::{HashMap, HashSet};

/// Positional arguments plus the flags that were given.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Parsed {
    pub pos: Vec<String>,
    values: HashMap<String, Vec<String>>,
    switches: HashSet<String>,
}

impl Parsed {
    /// Parse `args` against the declared flags (names include the dashes).
    pub fn parse(args: &[String], valued: &[&str], switches: &[&str]) -> Result<Parsed, String> {
        let mut out = Parsed::default();
        let mut it = args.iter();
        while let Some(arg) = it.next() {
            if arg == "--" {
                out.pos.extend(it.by_ref().cloned());
                break;
            }
            if arg == "-" || !arg.starts_with('-') {
                out.pos.push(arg.clone());
                continue;
            }
            let (name, inline) = match arg.split_once('=') {
                Some((n, v)) if n.starts_with("--") => (n, Some(v.to_string())),
                _ => (arg.as_str(), None),
            };
            if valued.contains(&name) {
                let value = match inline {
                    Some(v) => v,
                    None => it.next().cloned().ok_or(format!("{name} needs a value"))?,
                };
                out.values.entry(name.into()).or_default().push(value);
            } else if switches.contains(&name) && inline.is_none() {
                out.switches.insert(name.into());
            } else {
                return Err(format!("unknown flag {arg}"));
            }
        }
        Ok(out)
    }

    /// The last value given for `name`.
    pub fn value(&self, name: &str) -> Option<&str> {
        self.values.get(name)?.last().map(String::as_str)
    }

    /// Whether the switch `name` was given.
    pub fn has(&self, name: &str) -> bool {
        self.switches.contains(name)
    }

    /// Between `min` and `max` positionals, else the usage line as the error.
    pub fn positionals(&self, min: usize, max: usize, usage: &str) -> Result<&[String], String> {
        if (min..=max).contains(&self.pos.len()) {
            Ok(&self.pos)
        } else {
            Err(format!("usage: {usage}"))
        }
    }
}

/// Parse a numeric id argument.
pub fn parse_id(s: &str, what: &str) -> Result<u64, String> {
    s.trim()
        .parse()
        .map_err(|_| format!("not a {what} id: {s}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parses_flags_and_positionals() {
        let p = Parsed::parse(
            &v(&["a", "--due", "2026-01-02", "b", "--all", "--x=y", "-"]),
            &["--due", "--x"],
            &["--all"],
        )
        .unwrap();
        assert_eq!(p.pos, ["a", "b", "-"]);
        assert_eq!(p.value("--due"), Some("2026-01-02"));
        assert_eq!(p.value("--x"), Some("y"));
        assert!(p.has("--all"));
        assert!(!p.has("--none"));
    }

    #[test]
    fn rejects_unknown_and_missing_values() {
        assert!(Parsed::parse(&v(&["--nope"]), &[], &[]).is_err());
        assert!(Parsed::parse(&v(&["--due"]), &["--due"], &[]).is_err());
        assert!(Parsed::parse(&v(&["--all=1"]), &[], &["--all"]).is_err());
    }

    #[test]
    fn double_dash_ends_flags() {
        let p = Parsed::parse(&v(&["--", "--weird", "x"]), &[], &[]).unwrap();
        assert_eq!(p.pos, ["--weird", "x"]);
    }

    #[test]
    fn positional_count_is_checked() {
        let p = Parsed::parse(&v(&["a"]), &[], &[]).unwrap();
        assert!(p.positionals(1, 2, "u").is_ok());
        assert_eq!(
            p.positionals(2, 2, "locus x <a> <b>").unwrap_err(),
            "usage: locus x <a> <b>"
        );
    }

    #[test]
    fn ids_parse() {
        assert_eq!(parse_id(" 12 ", "note"), Ok(12));
        assert!(parse_id("x", "note").is_err());
    }
}
