//! The `draftstamp` commands (see `spec/0002_cli.md`).
//!
//! [`run`] holds every command and takes its surroundings (network, working directory, vault,
//! output streams) as parameters, so tests drive it in-process with a simulated network.

#![forbid(unsafe_code)]

pub mod http;

use std::ffi::OsString;
use std::fmt::Display;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};
use draftstamp_core::{
    Bundle, Error, Http, LocalVault, OtsError, SETTINGS_FILE, Settings, StampId, StampInfo,
    StampOptions, Status, Vault, default_project, find, history, iso_time,
};

/// Where a command runs: the working directory, the default vault, and the output streams.
pub struct Context<'a> {
    /// Relative paths are resolved against this.
    pub cwd: PathBuf,
    /// The vault used without `--vault` (`$DRAFTSTAMP_VAULT`, then `~/draftstamp`).
    pub default_vault: PathBuf,
    /// Results.
    pub out: &'a mut dyn Write,
    /// Errors, one sentence each.
    pub err: &'a mut dyn Write,
}

#[derive(Parser)]
#[command(
    name = "draftstamp",
    version,
    about = "Timestamp your drafts in Bitcoin and keep each one with its proof."
)]
struct Cli {
    /// The vault folder [default: $DRAFTSTAMP_VAULT, then ~/draftstamp]
    #[arg(long, global = true, value_name = "DIR")]
    vault: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Write .draftstamp in this folder, naming its project
    Init {
        /// Project name [default: this folder's name]
        #[arg(long)]
        project: Option<String>,
    },
    /// Stamp a folder (this one by default) or a single file
    Stamp {
        /// The folder or file to stamp
        path: Option<PathBuf>,
        /// A note about this draft
        #[arg(short = 'm', long = "message", value_name = "LABEL")]
        label: Option<String>,
        /// Tag this stamp with a project, instead of the one in .draftstamp
        #[arg(long)]
        project: Option<String>,
    },
    /// Complete pending proofs (all pending stamps if none is given)
    Upgrade {
        /// A stamp id, a unique prefix, or its 6-character ending
        stamp: Option<String>,
    },
    /// List stamps, newest first (this folder's project by default)
    Log {
        /// Only this project
        #[arg(long, conflicts_with = "all")]
        project: Option<String>,
        /// Every project
        #[arg(long)]
        all: bool,
    },
    /// Show the stamps that contain this file's exact bytes
    Find {
        /// The file to look for
        file: PathBuf,
    },
    /// Offline: check that a stored stamp is intact
    Check {
        /// A stamp id, a unique prefix, or its 6-character ending [default: newest of this project]
        stamp: Option<String>,
    },
    /// Check a stored stamp, then confirm its date with Bitcoin
    Verify {
        /// A stamp id, a unique prefix, or its 6-character ending [default: newest of this project]
        stamp: Option<String>,
    },
    /// Write files or folders from a stamp, each file with its own .ots proof
    Share {
        /// Paths inside the stamp: files (chapters/chapter3.tex) or folders (chapters/)
        #[arg(required = true)]
        paths: Vec<String>,
        /// Which stamp [default: the newest of this project containing them]
        #[arg(long)]
        stamp: Option<String>,
        /// Folder to write into [default: the current folder]
        #[arg(short = 'o', long = "out", value_name = "DIR", conflicts_with = "zip")]
        out: Option<PathBuf>,
        /// Write one zip instead: the files, their .ots and a README
        #[arg(long, value_name = "FILE")]
        zip: Option<PathBuf>,
    },
    /// Write one zip with everything, each file with its own .ots proof
    Export {
        /// A stamp id, a unique prefix, or its 6-character ending [default: newest of this project]
        stamp: Option<String>,
        /// The zip to write [default: <stamp id>.zip]
        #[arg(short = 'o', long = "out", value_name = "FILE")]
        out: Option<PathBuf>,
    },
}

/// Exit codes.
const FAILED: u8 = 1;
const USAGE: u8 = 2;
const DOES_NOT_VERIFY: u8 = 3;

/// A command's failure: one sentence and the exit code.
struct Failure {
    code: u8,
    message: String,
}

fn fail(message: impl Into<String>) -> Failure {
    Failure {
        code: FAILED,
        message: message.into(),
    }
}

impl From<Error> for Failure {
    fn from(e: Error) -> Self {
        let code = match &e {
            Error::Mismatch(_)
            | Error::InvalidReceipt(_)
            | Error::Ots(OtsError::HeaderMismatch | OtsError::Malformed(_)) => DOES_NOT_VERIFY,
            _ => FAILED,
        };
        Failure {
            code,
            message: sentence(&e.to_string()),
        }
    }
}

impl From<std::io::Error> for Failure {
    fn from(e: std::io::Error) -> Self {
        Failure::from(Error::Io(e))
    }
}

/// Capitalized, ending with a full stop.
fn sentence(s: &str) -> String {
    let mut c = s.chars();
    let mut out: String = c
        .next()
        .map(|f| f.to_uppercase().chain(c).collect())
        .unwrap_or_default();
    if !out.ends_with('.') {
        out.push('.');
    }
    out
}

/// Runs one command line (including the program name) and returns the exit code.
pub fn run<I, T>(args: I, http: &impl Http, ctx: &mut Context) -> u8
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let cli = match Cli::try_parse_from(args) {
        Ok(cli) => cli,
        Err(e) => {
            let (stream, code): (&mut dyn Write, u8) = if e.use_stderr() {
                (&mut *ctx.err, USAGE)
            } else {
                (&mut *ctx.out, 0)
            };
            let _ = write!(stream, "{}", e.render());
            return code;
        }
    };
    let vault_root = match cli.vault {
        Some(v) => ctx.cwd.join(v),
        None => ctx.default_vault.clone(),
    };
    let mut app = App {
        http,
        cwd: &ctx.cwd,
        vault_root,
        out: &mut *ctx.out,
        err: &mut *ctx.err,
    };
    match app.dispatch(cli.command) {
        Ok(()) => 0,
        Err(f) => {
            let _ = writeln!(app.err, "{}", f.message);
            f.code
        }
    }
}

struct App<'a, H> {
    http: &'a H,
    cwd: &'a Path,
    vault_root: PathBuf,
    out: &'a mut dyn Write,
    err: &'a mut dyn Write,
}

type Done = Result<(), Failure>;

impl<H: Http> App<'_, H> {
    fn say(&mut self, line: impl Display) {
        let _ = writeln!(self.out, "{line}");
    }

    fn dispatch(&mut self, command: Command) -> Done {
        match command {
            Command::Init { project } => self.init(project),
            Command::Stamp {
                path,
                label,
                project,
            } => self.stamp(path, label, project),
            Command::Upgrade { stamp } => self.upgrade(stamp),
            Command::Log { project, all } => self.log(project, all),
            Command::Find { file } => self.find(&file),
            Command::Check { stamp } => self.check(stamp),
            Command::Verify { stamp } => self.verify(stamp),
            Command::Share {
                paths,
                stamp,
                out,
                zip,
            } => self.share(&paths, stamp, out, zip),
            Command::Export { stamp, out } => self.export(stamp, out),
        }
    }

    // ---- the vault and stamp references ----

    /// The vault, if it exists. Read-only commands never create it.
    fn existing_vault(&self) -> Result<Option<LocalVault>, Failure> {
        if self.vault_root.is_dir() {
            Ok(Some(LocalVault::open(&self.vault_root)?))
        } else {
            Ok(None)
        }
    }

    fn vault(&self) -> Result<LocalVault, Failure> {
        self.existing_vault()?.ok_or_else(|| {
            fail(format!(
                "No vault at {} yet; stamp something first.",
                self.vault_root.display()
            ))
        })
    }

    fn current_project(&self) -> Result<Option<String>, Failure> {
        Ok(default_project(self.cwd)?)
    }

    /// A full id, a unique prefix, or the 6-character ending; omitted, the newest stamp of the
    /// current folder's project.
    fn resolve(&self, vault: &LocalVault, reference: Option<&str>) -> Result<StampId, Failure> {
        let Some(r) = reference.filter(|r| !r.is_empty()) else {
            let project = self.current_project()?;
            return history(vault, project.as_deref())?
                .into_iter()
                .next()
                .map(|s| s.id)
                .ok_or_else(|| fail(no_stamps(project.as_deref())));
        };
        let ids = vault.list()?;
        if let Some(id) = ids.iter().find(|id| id.as_str() == r) {
            return Ok(id.clone());
        }
        let matches: Vec<&StampId> = ids
            .iter()
            .filter(|id| id.as_str().starts_with(r) || id.as_str().rsplit('-').next() == Some(r))
            .collect();
        match matches.as_slice() {
            [] => Err(fail(format!("No stamp matches \"{r}\"."))),
            [one] => Ok((*one).clone()),
            many => Err(fail(format!(
                "\"{r}\" matches several stamps: {}. Use more characters.",
                many.iter()
                    .map(|i| i.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ))),
        }
    }

    // ---- commands ----

    fn init(&mut self, project: Option<String>) -> Done {
        let path = self.cwd.join(SETTINGS_FILE);
        if path.exists() {
            return Err(fail(
                ".draftstamp already exists here; edit it to change the project.",
            ));
        }
        let project = match project {
            Some(p) => p.trim().to_owned(),
            None => self
                .cwd
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_owned(),
        };
        if project.is_empty() || project.contains(['\n', '\r']) {
            return Err(fail("A project name must be a single, non-empty line."));
        }
        let settings = Settings {
            project: Some(project.clone()),
        };
        fs::write(&path, settings.to_text())?;
        self.say(format!(
            "Wrote .draftstamp: stamps of this folder are tagged \"{project}\"."
        ));
        Ok(())
    }

    fn stamp(
        &mut self,
        path: Option<PathBuf>,
        label: Option<String>,
        project: Option<String>,
    ) -> Done {
        let target = path.map_or_else(|| self.cwd.to_path_buf(), |p| self.cwd.join(p));
        if !target.exists() {
            return Err(fail(format!(
                "No such file or folder: {}.",
                target.display()
            )));
        }
        if let Some(p) = &project {
            let folder = if target.is_dir() {
                target.clone()
            } else {
                target.parent().map(Path::to_path_buf).unwrap_or_default()
            };
            if let Some(configured) = Settings::load(&folder)?.project
                && configured != *p
            {
                self.say(format!(
                    "Note: tagging this stamp \"{p}\", but .draftstamp says \"{configured}\". Earlier stamps keep their own tags."
                ));
            }
        }
        let bundle = Bundle::create(self.http, &target, &StampOptions { label, project })?;
        let id = LocalVault::open(&self.vault_root)?.put(&bundle)?;
        let files = bundle.receipt().committed().leaves().len();
        let of = bundle
            .receipt()
            .advisory()
            .project
            .as_ref()
            .map(|p| format!(" of {p}"))
            .unwrap_or_default();
        self.say(format!("Stamped {}{of} as {id}.", count(files, "file")));
        self.say("Pending: Bitcoin confirmation usually takes a few hours. Run `draftstamp upgrade` later.");
        Ok(())
    }

    fn upgrade(&mut self, reference: Option<String>) -> Done {
        let Some(vault) = self.existing_vault()? else {
            self.say("Nothing pending.");
            return Ok(());
        };
        let ids = match reference {
            Some(r) => vec![self.resolve(&vault, Some(&r))?],
            None => {
                let mut pending = Vec::new();
                for id in vault.list()? {
                    if vault.proof(&id)?.status() == Status::Pending {
                        pending.push(id);
                    }
                }
                pending
            }
        };
        if ids.is_empty() {
            self.say("Nothing pending.");
            return Ok(());
        }
        let mut code = 0;
        for id in ids {
            match upgrade_one(self.http, &vault, &id) {
                Ok(Status::Pending) => self.say(format!(
                    "{id}  still pending; Bitcoin has not confirmed it yet. Try again later."
                )),
                Ok(status) => self.say(format!("{id}  {}", describe(status))),
                Err(f) => {
                    let _ = writeln!(self.err, "{id}: {}", f.message);
                    code = code.max(f.code);
                }
            }
        }
        if code == 0 {
            Ok(())
        } else {
            Err(Failure {
                code,
                message: "Some stamps could not be upgraded.".into(),
            })
        }
    }

    fn log(&mut self, project: Option<String>, all: bool) -> Done {
        let filter = if all {
            None
        } else {
            project.or(self.current_project()?)
        };
        let stamps = match self.existing_vault()? {
            Some(vault) => history(&vault, filter.as_deref())?,
            None => Vec::new(),
        };
        if stamps.is_empty() {
            let mut message = no_stamps(filter.as_deref());
            if filter.is_some() {
                message.push_str(" Use --all to see every project.");
            }
            self.say(message);
        }
        for s in stamps {
            self.say(line(&s));
        }
        Ok(())
    }

    fn find(&mut self, file: &Path) -> Done {
        let file = self.cwd.join(file);
        if !file.is_file() {
            return Err(fail(format!("No such file: {}.", file.display())));
        }
        let found = match self.existing_vault()? {
            Some(vault) => find(&vault, &file)?,
            None => Vec::new(),
        };
        if found.is_empty() {
            self.say("No stamp contains this exact file.");
        }
        for f in found {
            self.say(format!(
                "{}  {}  {}  {}",
                f.stamp.id,
                f.path,
                f.stamp.project.as_deref().unwrap_or("-"),
                describe(f.stamp.status)
            ));
        }
        Ok(())
    }

    fn check(&mut self, reference: Option<String>) -> Done {
        let vault = self.vault()?;
        let id = self.resolve(&vault, reference.as_deref())?;
        let files = vault.get(&id)?.check()?;
        self.say(intact(files));
        Ok(())
    }

    fn verify(&mut self, reference: Option<String>) -> Done {
        let vault = self.vault()?;
        let id = self.resolve(&vault, reference.as_deref())?;
        let bundle = vault.get(&id)?;
        let files = bundle.check()?;
        self.say(intact(files));
        match bundle.verify(self.http)?.status {
            Status::Anchored { height, time } => {
                self.say(format!(
                    "Bitcoin: confirmed in block {height}. These files existed by {}.",
                    date(time)
                ));
                Ok(())
            }
            Status::Pending => {
                self.say("Bitcoin: pending. Run `draftstamp upgrade` once Bitcoin confirms it, usually within a few hours.");
                Ok(())
            }
            Status::Claimed { height } => Err(fail(format!(
                "Bitcoin: the proof names block {height}, but the block explorers could not confirm it. Try again later."
            ))),
        }
    }

    fn share(
        &mut self,
        paths: &[String],
        reference: Option<String>,
        out: Option<PathBuf>,
        zip: Option<PathBuf>,
    ) -> Done {
        let paths: Vec<&str> = paths.iter().map(String::as_str).collect();
        let vault = self.vault()?;
        let id = match reference {
            Some(r) => self.resolve(&vault, Some(&r))?,
            None => {
                let project = self.current_project()?;
                let mut newest = None;
                for s in history(&vault, project.as_deref())? {
                    if vault.receipt(&s.id)?.select(&paths).is_ok() {
                        newest = Some(s.id);
                        break;
                    }
                }
                newest.ok_or_else(|| {
                    fail(format!(
                        "No stamp of {} contains {}. Use paths as `draftstamp find` shows them, or --stamp.",
                        project.map_or("this project".to_owned(), |p| format!("\"{p}\"")),
                        paths.join(", ")
                    ))
                })?
            }
        };
        let mut bundle = vault.get(&id)?;
        if bundle.proof().status() == Status::Pending {
            let before = bundle.proof().clone();
            match bundle.upgrade(self.http) {
                Ok(_) if *bundle.proof() != before => vault.put_proof(&id, bundle.proof())?,
                Ok(_) => {}
                Err(e) => self.say(format!(
                    "Note: could not upgrade the proof ({e}); sharing it as it is."
                )),
            }
        }
        let pending = bundle.proof().status() == Status::Pending;

        if let Some(zip) = zip {
            let zip = self.cwd.join(zip);
            if zip.exists() {
                return Err(fail(format!("{} already exists.", zip.display())));
            }
            let files = bundle.share_zip(&paths, &zip)?;
            self.say(format!(
                "Wrote {}: {}, each with its own .ots, and a README, from stamp {id}.",
                zip.display(),
                count(files, "file")
            ));
            self.share_footer(pending, "<file>");
            return Ok(());
        }

        let selected = bundle.select(&paths)?;
        let dir = out.map_or_else(|| self.cwd.to_path_buf(), |o| self.cwd.join(o));
        // One file is written by name; several keep their folders.
        let target = |path: &str| -> PathBuf {
            if selected.len() == 1 {
                dir.join(path.rsplit('/').next().unwrap_or(path))
            } else {
                dir.join(path)
            }
        };
        let mut shared = Vec::new();
        for path in &selected {
            let file = bundle.share(path)?;
            let (file_out, ots_out) = (target(path), target(&format!("{path}.ots")));
            if ots_out.exists() {
                return Err(fail(format!(
                    "{} already exists; choose another folder with -o.",
                    ots_out.display()
                )));
            }
            // The file may already be here (sharing from the project folder); only identical bytes.
            match fs::read(&file_out) {
                Ok(existing) if existing != file.bytes => {
                    return Err(fail(format!(
                        "{} already exists with different contents; choose another folder with -o.",
                        file_out.display()
                    )));
                }
                Ok(_) => shared.push((file, file_out, ots_out, false)),
                Err(_) => shared.push((file, file_out, ots_out, true)),
            }
        }
        for (file, file_out, ots_out, write_file) in &shared {
            if let Some(parent) = file_out.parent() {
                fs::create_dir_all(parent)?;
            }
            if *write_file {
                fs::write(file_out, &file.bytes)?;
            }
            fs::write(ots_out, file.proof.to_bytes())?;
        }
        if let [(_, _, _, write_file)] = shared.as_slice() {
            let name = selected[0].rsplit('/').next().unwrap_or(&selected[0]);
            let wrote = if *write_file {
                format!("Wrote {name} and {name}.ots")
            } else {
                format!("Wrote {name}.ots")
            };
            self.say(format!("{wrote} from stamp {id}."));
            self.share_footer(pending, name);
        } else {
            self.say(format!(
                "Wrote {} and their .ots into {} from stamp {id}:",
                count(selected.len(), "file"),
                dir.display()
            ));
            for path in &selected {
                self.say(format!("  {path}"));
            }
            self.share_footer(pending, "<file>");
        }
        Ok(())
    }

    fn share_footer(&mut self, pending: bool, name: &str) {
        if pending {
            self.say("The proofs are still pending: the recipient's `ots verify` completes them once Bitcoin confirms, or share again after `draftstamp upgrade`.");
        }
        self.say(format!(
            "Anyone can check it with: ots verify -f {name} {name}.ots"
        ));
    }

    fn export(&mut self, reference: Option<String>, out: Option<PathBuf>) -> Done {
        let vault = self.vault()?;
        let id = self.resolve(&vault, reference.as_deref())?;
        let out = out.map_or_else(|| self.cwd.join(format!("{id}.zip")), |o| self.cwd.join(o));
        if out.exists() {
            return Err(fail(format!("{} already exists.", out.display())));
        }
        let bundle = vault.get(&id)?;
        bundle.export(&out)?;
        let files = bundle.receipt().committed().leaves().len();
        self.say(format!(
            "Wrote {}: {}, each with its own .ots, and a README on how to check them.",
            out.display(),
            count(files, "file")
        ));
        Ok(())
    }
}

/// Upgrades one stored stamp and saves the proof if it changed.
fn upgrade_one(http: &impl Http, vault: &LocalVault, id: &StampId) -> Result<Status, Failure> {
    let mut bundle = vault.get(id)?;
    let before = bundle.proof().clone();
    let status = bundle.upgrade(http)?;
    if *bundle.proof() != before {
        vault.put_proof(id, bundle.proof())?;
    }
    Ok(status)
}

fn no_stamps(project: Option<&str>) -> String {
    match project {
        Some(p) => format!("No stamps for project \"{p}\" yet."),
        None => "No stamps yet.".to_owned(),
    }
}

fn count(n: usize, noun: &str) -> String {
    if n == 1 {
        format!("1 {noun}")
    } else {
        format!("{n} {noun}s")
    }
}

/// `2026-11-21 16:02 UTC`
fn date(unix_seconds: u32) -> String {
    let iso = iso_time(unix_seconds.into());
    format!("{} {} UTC", &iso[..10], &iso[11..16])
}

fn describe(status: Status) -> String {
    match status {
        Status::Pending => "pending".to_owned(),
        Status::Claimed { height } => format!("in Bitcoin block {height}"),
        Status::Anchored { height, time } => {
            format!("confirmed in Bitcoin block {height} ({})", date(time))
        }
    }
}

fn intact(files: usize) -> String {
    format!(
        "Record intact: {} the receipt, and the proof is for this stamp.",
        if files == 1 {
            "the file matches".to_owned()
        } else {
            format!("all {files} files match")
        }
    )
}

fn line(s: &StampInfo) -> String {
    let mut line = format!(
        "{}  {}  {}  {}",
        s.id,
        s.project.as_deref().unwrap_or("-"),
        count(s.files, "file"),
        describe(s.status)
    );
    if let Some(label) = &s.label {
        line.push_str(&format!("  \"{label}\""));
    }
    line
}
