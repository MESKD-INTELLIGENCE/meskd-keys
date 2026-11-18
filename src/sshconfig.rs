use std::fs;
use std::path::Path;

use crate::error::{Error, Result};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostEntry {
    pub alias: String,
    pub hostname: String,
    pub user: String,
    pub port: String,
    pub identity: String,
    start: usize,
    end: usize,
}

#[derive(Clone, Debug, Default)]
pub struct NewHost {
    pub alias: String,
    pub hostname: String,
    pub user: String,
    pub port: String,
    pub identity: String,
}

impl HostEntry {
    pub fn target(&self) -> String {
        let host = if self.hostname.is_empty() {
            &self.alias
        } else {
            &self.hostname
        };
        if self.user.is_empty() {
            host.to_owned()
        } else {
            format!("{}@{host}", self.user)
        }
    }

    pub fn ssh_command(&self) -> String {
        let mut command = format!("ssh {}", self.alias);
        if !self.port.is_empty() && self.port != "22" {
            command.push_str(&format!(" -p {}", self.port));
        }
        command
    }
}

pub fn config_path(dir: &Path) -> std::path::PathBuf {
    dir.join("config")
}

pub fn parse(text: &str) -> Vec<HostEntry> {
    let lines: Vec<&str> = text.lines().collect();
    let mut hosts = Vec::new();
    let mut at = 0;
    while at < lines.len() {
        let line = strip_comment(lines[at]).trim().to_owned();
        if line.is_empty() {
            at += 1;
            continue;
        }
        let (key, value) = split(&line);
        if !key.eq_ignore_ascii_case("host") {
            at += 1;
            continue;
        }
        let start = at;
        let mut end = at + 1;
        while end < lines.len() {
            let next = strip_comment(lines[end]).trim().to_owned();
            if next.is_empty() {
                end += 1;
                continue;
            }
            let (next_key, _) = split(&next);
            if next_key.eq_ignore_ascii_case("host") || next_key.eq_ignore_ascii_case("match") {
                break;
            }
            end += 1;
        }
        let mut host = HostEntry {
            alias: value.split_whitespace().collect::<Vec<_>>().join(" "),
            hostname: String::new(),
            user: String::new(),
            port: String::new(),
            identity: String::new(),
            start,
            end,
        };
        for line in &lines[start + 1..end] {
            let (key, value) = split(strip_comment(line).trim());
            if value.is_empty() {
                continue;
            }
            if key.eq_ignore_ascii_case("hostname") && host.hostname.is_empty() {
                host.hostname = value.to_owned();
            } else if key.eq_ignore_ascii_case("user") && host.user.is_empty() {
                host.user = value.to_owned();
            } else if key.eq_ignore_ascii_case("port") && host.port.is_empty() {
                host.port = value.to_owned();
            } else if key.eq_ignore_ascii_case("identityfile") && host.identity.is_empty() {
                host.identity = value.to_owned();
            }
        }
        if !host.alias.is_empty() {
            hosts.push(host);
        }
        at = end;
    }
    hosts
}

pub fn add(dir: &Path, host: &NewHost) -> Result<()> {
    validate(host)?;
    let path = config_path(dir);
    let existing = fs::read_to_string(&path).unwrap_or_default();
    if parse(&existing).iter().any(|h| h.alias == host.alias) {
        return Err(Error::HostExists);
    }
    let mut text = existing;
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    if !text.is_empty() && !text.ends_with("\n\n") {
        text.push('\n');
    }
    text.push_str(&format!("Host {}\n", host.alias));
    if !host.hostname.trim().is_empty() {
        text.push_str(&format!("    HostName {}\n", host.hostname.trim()));
    }
    if !host.user.trim().is_empty() {
        text.push_str(&format!("    User {}\n", host.user.trim()));
    }
    if !host.port.trim().is_empty() && host.port.trim() != "22" {
        text.push_str(&format!("    Port {}\n", host.port.trim()));
    }
    if !host.identity.trim().is_empty() {
        text.push_str(&format!("    IdentityFile {}\n", host.identity.trim()));
    }
    write_private(&path, text.as_bytes())
}

pub fn remove(dir: &Path, alias: &str) -> Result<()> {
    let path = config_path(dir);
    let text = fs::read_to_string(&path)?;
    let lines: Vec<&str> = text.lines().collect();
    let hosts = parse(&text);
    let matches: Vec<&HostEntry> = hosts.iter().filter(|h| h.alias == alias).collect();
    if matches.len() != 1 {
        return Err(Error::NotFound);
    }
    let block = matches[0];
    let mut end = block.end;
    if end < lines.len() && lines[end].trim().is_empty() {
        end += 1;
    }
    let mut kept: Vec<&str> = Vec::new();
    kept.extend_from_slice(&lines[..block.start]);
    kept.extend_from_slice(&lines[end..]);
    let mut result = kept.join("\n");
    if !result.is_empty() && !result.ends_with('\n') {
        result.push('\n');
    }
    write_private(&path, result.as_bytes())
}

fn validate(host: &NewHost) -> Result<()> {
    let alias = host.alias.trim();
    let ok = !alias.is_empty()
        && alias.len() <= 64
        && !alias.contains(char::is_whitespace)
        && !alias.contains('#')
        && !alias.contains('"');
    if ok { Ok(()) } else { Err(Error::InvalidAlias) }
}

fn split(line: &str) -> (&str, &str) {
    match line.find(|c: char| c.is_whitespace() || c == '=') {
        Some(at) => {
            let key = &line[..at];
            let value = line[at..].trim_start_matches(|c: char| c.is_whitespace() || c == '=');
            (key, value.trim())
        }
        None => (line, ""),
    }
}

fn strip_comment(line: &str) -> &str {
    match line.find('#') {
        Some(at) if at == 0 || line[..at].ends_with(|c: char| c.is_whitespace()) => &line[..at],
        _ => line,
    }
}

fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = std::path::PathBuf::from(tmp);
    fs::write(&tmp, bytes)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600))?;
    }
    fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "# my hosts\n\
Host prod\n\
    HostName prod.example.com\n\
    User deploy\n\
    Port 2222\n\
    IdentityFile ~/.ssh/id_prod\n\n\
Host db db-alt\n\
    HostName db.example.com\n\
    User admin\n\n\
Host *\n\
    ServerAliveInterval 60\n";

    #[test]
    fn parses_blocks_with_comments_and_wildcards() {
        let hosts = parse(SAMPLE);
        assert_eq!(hosts.len(), 3);
        assert_eq!(hosts[0].alias, "prod");
        assert_eq!(hosts[0].hostname, "prod.example.com");
        assert_eq!(hosts[0].user, "deploy");
        assert_eq!(hosts[0].port, "2222");
        assert_eq!(hosts[0].identity, "~/.ssh/id_prod");
        assert_eq!(hosts[1].alias, "db db-alt");
        assert_eq!(hosts[1].port, "");
        assert_eq!(hosts[2].alias, "*");
        assert_eq!(hosts[0].ssh_command(), "ssh prod -p 2222");
        assert_eq!(hosts[1].target(), "admin@db.example.com");
    }

    #[test]
    fn adds_a_host_to_an_empty_config() {
        let dir = tempfile::tempdir().unwrap();
        add(
            dir.path(),
            &NewHost {
                alias: "vps".to_owned(),
                hostname: "203.0.113.5".to_owned(),
                user: "root".to_owned(),
                port: "22".to_owned(),
                identity: "~/.ssh/id_vps".to_owned(),
            },
        )
        .unwrap();
        let text = fs::read_to_string(dir.path().join("config")).unwrap();
        assert_eq!(
            text,
            "Host vps\n    HostName 203.0.113.5\n    User root\n    IdentityFile ~/.ssh/id_vps\n"
        );
        assert!(matches!(
            add(
                dir.path(),
                &NewHost {
                    alias: "vps".to_owned(),
                    ..NewHost::default()
                }
            ),
            Err(Error::HostExists)
        ));
    }

    #[test]
    fn appends_and_removes_without_touching_other_lines() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("config"), SAMPLE).unwrap();
        add(
            dir.path(),
            &NewHost {
                alias: "new".to_owned(),
                hostname: "new.example.com".to_owned(),
                user: "me".to_owned(),
                port: "2200".to_owned(),
                identity: String::new(),
            },
        )
        .unwrap();
        let text = fs::read_to_string(dir.path().join("config")).unwrap();
        assert!(text.contains("Host prod\n"));
        assert!(
            text.contains("Host new\n    HostName new.example.com\n    User me\n    Port 2200\n")
        );
        assert!(text.contains("# my hosts"));

        remove(dir.path(), "new").unwrap();
        let text = fs::read_to_string(dir.path().join("config")).unwrap();
        assert!(!text.contains("Host new"));
        assert!(text.contains("Host prod\n"));
        assert!(text.contains("Host *\n"));

        remove(dir.path(), "prod").unwrap();
        let text = fs::read_to_string(dir.path().join("config")).unwrap();
        assert!(!text.contains("Host prod"));
        assert!(text.contains("Host db db-alt\n"));
        assert!(text.contains("Host *\n"));
        assert_eq!(parse(&text).len(), 2);
    }

    #[test]
    fn removes_the_right_block_when_aliases_repeat() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("config"),
            "Host x\n    HostName one\n\nHost y\n    HostName two\n\nHost x\n    HostName three\n",
        )
        .unwrap();
        assert!(matches!(remove(dir.path(), "x"), Err(Error::NotFound)));
    }

    #[test]
    fn rejects_bad_aliases() {
        let dir = tempfile::tempdir().unwrap();
        for alias in ["", "two words", "with#hash"] {
            assert!(matches!(
                add(
                    dir.path(),
                    &NewHost {
                        alias: alias.to_owned(),
                        ..NewHost::default()
                    }
                ),
                Err(Error::InvalidAlias)
            ));
        }
    }

    #[test]
    fn equal_signs_and_case_are_accepted() {
        let hosts = parse("host Work\n  hostname=work.example.com\n  user = me\n  PORT= 2200\n");
        assert_eq!(hosts.len(), 1);
        assert_eq!(hosts[0].hostname, "work.example.com");
        assert_eq!(hosts[0].user, "me");
        assert_eq!(hosts[0].port, "2200");
    }

    #[cfg(unix)]
    #[test]
    fn config_is_written_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        add(
            dir.path(),
            &NewHost {
                alias: "a".to_owned(),
                ..NewHost::default()
            },
        )
        .unwrap();
        let mode = fs::metadata(dir.path().join("config"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
    }
}
